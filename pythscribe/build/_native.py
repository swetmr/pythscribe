"""ONE authority for the native-binary facts the wheel contract keys on (spec 13-09-26, M1.2 + M1.3).

Three consumers, one parser -- so they can never disagree:
  * `find_pyths()` (M1.2): "is this candidate a NATIVE executable (ELF / Mach-O / PE), not a text
    shebang wrapper?" + the install-time arch self-check ("a wheel for another platform was
    installed" is a clear error, never a SIGILL / `Exec format error`).
  * `scripts/wheel_passthrough.py` (M1.3, the repair-hook gate): the binary's REAL floor vs the
    wheel tag -- glibc symbol versions / DT_NEEDED for `manylinux_2_28_*`, `LC_BUILD_VERSION`
    minos + cputype + dylib set for `macosx_11_0_*`, PE `Machine` + console subsystem for
    `win_amd64` (rev-7: no OS-version floor is claimed for Windows).
  * `pythscribe_wheel_backend.py` (the PEP 517 hook): the wheel tag for the host / the staged
    binary, and the cheap format+arch consistency check at build time.

Dependency-free by design (stdlib `struct` only): the parser must run inside cibuildwheel's
repair step on every leg, inside the manylinux container, and at `pip install` time in a user's
venv. `auditwheel show` / `otool -l` remain CORROBORATING tool checks in the passthrough script
(`--require-auditwheel` / `--require-otool`); the floor decision itself is made here from the
bytes, so the three synthesized fixtures of validation §I6 (GLIBC_2.31-in-2_28, minos-12-in-11_0,
arm64-in-x86_64) are RED on every host, not only on the leg's OS.
"""
from __future__ import annotations

import platform
import re
import struct
from dataclasses import dataclass

__all__ = [
    "EXPECTED_WHEEL_SET", "TRIPLE_TO_TAG", "NativeFormatError", "NativeInfo", "sniff_format",
    "inspect_binary", "check_floor", "check_format_and_arch", "tag_requirements", "host_wheel_tag",
    "host_expectation", "host_mismatch",
]

# The complete expected wheel set (requirements §6): exactly the 5 release.yml targets. A missing
# member is RED in scripts/verify_wheel_set.py; a tag outside this set is refused by the
# passthrough. Windows-ARM64 / musl are documented gaps.
TRIPLE_TO_TAG: dict[str, str] = {
    "x86_64-unknown-linux-gnu": "manylinux_2_28_x86_64",
    "aarch64-unknown-linux-gnu": "manylinux_2_28_aarch64",
    "x86_64-apple-darwin": "macosx_11_0_x86_64",
    "aarch64-apple-darwin": "macosx_11_0_arm64",
    "x86_64-pc-windows-msvc": "win_amd64",
}
EXPECTED_WHEEL_SET: frozenset[str] = frozenset(TRIPLE_TO_TAG.values())

# tag -> (format, normalized arch, floor). Floors: glibc (major, minor) for manylinux; macOS
# minos (major, minor, patch); None for win_amd64 (no OS floor is asserted -- rev-7).
_TAG_RULES: dict[str, tuple[str, str, tuple[int, ...] | None]] = {
    "win_amd64": ("pe", "x86_64", None),
    "manylinux_2_28_x86_64": ("elf", "x86_64", (2, 28)),
    "manylinux_2_28_aarch64": ("elf", "aarch64", (2, 28)),
    "macosx_11_0_x86_64": ("macho", "x86_64", (11, 0, 0)),
    "macosx_11_0_arm64": ("macho", "aarch64", (11, 0, 0)),
}

# manylinux_2_28 shared-library allowlist (the subset a Rust `-gnu` executable can legitimately
# need; auditwheel's policy list is wider -- X11/GL/glib -- but a compiler needs none of those).
MANYLINUX_2_28_ALLOWED_LIBS: frozenset[str] = frozenset({
    "libc.so.6", "libm.so.6", "libdl.so.2", "libpthread.so.0", "librt.so.1", "libgcc_s.so.1",
    "libstdc++.so.6", "libutil.so.1", "libresolv.so.2", "libnsl.so.1", "libz.so.1",
})
MACOS_SYSTEM_DYLIB_PREFIXES: tuple[str, ...] = ("/usr/lib/", "/System/")

_GLIBC_VER = re.compile(r"^GLIBC_(\d+)\.(\d+)(?:\.\d+)?$")


class NativeFormatError(ValueError):
    """Not a native executable this contract recognises (a text shebang wrapper, a truncated or
    foreign-format file, an unsupported ELF class / byte order)."""


@dataclass(frozen=True)
class NativeInfo:
    fmt: str                     # "pe" | "elf" | "macho"
    arch: str                    # "x86_64" | "aarch64" | "i686" | "fat" | "unknown(<n>)"
    pe_subsystem: int | None = None
    pe_characteristics: int = 0
    elf_needed: tuple[str, ...] = ()
    elf_version_needs: tuple[str, ...] = ()   # every verneed aux name, e.g. "GLIBC_2.28", "GCC_3.0"
    elf_glibc_max: tuple[int, int] | None = None
    macho_minos: tuple[int, int, int] | None = None
    macho_dylibs: tuple[str, ...] = ()
    macho_fat: bool = False

    @property
    def describe(self) -> str:
        if self.fmt == "elf":
            g = ".".join(map(str, self.elf_glibc_max)) if self.elf_glibc_max else "none"
            return f"ELF {self.arch}, max GLIBC_{g}, needs {list(self.elf_needed)}"
        if self.fmt == "macho":
            m = ".".join(map(str, self.macho_minos)) if self.macho_minos else "none"
            return f"Mach-O {self.arch}{' FAT' if self.macho_fat else ''}, minos {m}, dylibs {list(self.macho_dylibs)}"
        return f"PE {self.arch}, subsystem {self.pe_subsystem}, characteristics 0x{self.pe_characteristics:04x}"


# ----------------------------------------------------------------------------- magic / sniffing

_MACHO_MAGICS = {
    b"\xcf\xfa\xed\xfe": ("macho64", "<"), b"\xfe\xed\xfa\xcf": ("macho64", ">"),
    b"\xce\xfa\xed\xfe": ("macho32", "<"), b"\xfe\xed\xfa\xce": ("macho32", ">"),
    b"\xca\xfe\xba\xbe": ("fat", ">"),
}


def sniff_format(head: bytes) -> str | None:
    """'pe' | 'elf' | 'macho' from the first bytes, or None for anything else (a `#!` text wrapper,
    a `.py`, an empty file). No parsing beyond the magic -- this is the cheap pre-filter
    `find_pyths()` applies before it would EXECUTE a PATH candidate."""
    if head[:2] == b"MZ":
        return "pe"
    if head[:4] == b"\x7fELF":
        return "elf"
    if head[:4] in _MACHO_MAGICS:
        return "macho"
    return None


# ----------------------------------------------------------------------------- parsers

def _u16(b: bytes, off: int, order: str = "<") -> int:
    return struct.unpack_from(order + "H", b, off)[0]


def _u32(b: bytes, off: int, order: str = "<") -> int:
    return struct.unpack_from(order + "I", b, off)[0]


def _u64(b: bytes, off: int, order: str = "<") -> int:
    return struct.unpack_from(order + "Q", b, off)[0]


def _cstr(b: bytes, off: int) -> str:
    end = b.find(b"\x00", off)
    if end < 0 or off < 0 or off >= len(b):
        raise NativeFormatError(f"string at offset {off} runs past the end of the file")
    return b[off:end].decode("utf-8", "replace")


def _parse_pe(b: bytes) -> NativeInfo:
    try:
        e_lfanew = _u32(b, 0x3C)
        if b[e_lfanew:e_lfanew + 4] != b"PE\x00\x00":
            raise NativeFormatError("MZ header without a PE signature (a DOS stub / not a PE image)")
        machine = _u16(b, e_lfanew + 4)
        size_opt = _u16(b, e_lfanew + 20)
        characteristics = _u16(b, e_lfanew + 22)
        opt = e_lfanew + 24
        subsystem = _u16(b, opt + 68) if size_opt >= 70 else None
    except struct.error as e:
        raise NativeFormatError(f"truncated PE header: {e}") from None
    arch = {0x8664: "x86_64", 0xAA64: "aarch64", 0x014C: "i686"}.get(machine, f"unknown(0x{machine:04x})")
    return NativeInfo(fmt="pe", arch=arch, pe_subsystem=subsystem, pe_characteristics=characteristics)


def _parse_elf(b: bytes) -> NativeInfo:
    """ELF64 little-endian only (both Linux targets). DT_NEEDED + the GLIBC_x.y version needs are
    read from PT_DYNAMIC (DT_STRTAB / DT_VERNEED / DT_VERNEEDNUM), mapping virtual addresses to
    file offsets through the PT_LOAD segments -- the same data `auditwheel show` derives its
    policy from, without the dependency."""
    if b[4] != 2:
        raise NativeFormatError("ELF32 is not a supported release format (targets are 64-bit)")
    if b[5] != 1:
        raise NativeFormatError("big-endian ELF is not a supported release format")
    try:
        e_machine = _u16(b, 18)
        e_phoff = _u64(b, 32)
        e_phentsize = _u16(b, 54)
        e_phnum = _u16(b, 56)
        loads: list[tuple[int, int, int]] = []  # (vaddr, filesz, offset)
        dynamic: tuple[int, int] | None = None   # (offset, filesz)
        for i in range(e_phnum):
            ph = e_phoff + i * e_phentsize
            p_type = _u32(b, ph)
            p_offset, p_vaddr = _u64(b, ph + 8), _u64(b, ph + 16)
            p_filesz = _u64(b, ph + 32)
            if p_type == 1:
                loads.append((p_vaddr, p_filesz, p_offset))
            elif p_type == 2:
                dynamic = (p_offset, p_filesz)
    except (struct.error, IndexError) as e:
        raise NativeFormatError(f"truncated ELF header: {e}") from None
    arch = {0x3E: "x86_64", 0xB7: "aarch64", 0x03: "i686"}.get(e_machine, f"unknown(0x{e_machine:04x})")

    def v2o(vaddr: int) -> int:
        for lv, lsz, lo in loads:
            if lv <= vaddr < lv + lsz:
                return lo + (vaddr - lv)
        raise NativeFormatError(f"virtual address 0x{vaddr:x} is not inside any PT_LOAD segment")

    needed: list[str] = []
    version_needs: list[str] = []
    glibc_max: tuple[int, int] | None = None
    if dynamic is not None:
        d_off, d_sz = dynamic
        entries: list[tuple[int, int]] = []
        try:
            for off in range(d_off, d_off + d_sz, 16):
                tag, val = struct.unpack_from("<qQ", b, off)
                if tag == 0:
                    break
                entries.append((tag, val))
        except struct.error as e:
            raise NativeFormatError(f"truncated PT_DYNAMIC: {e}") from None
        tags = dict(entries)
        strtab_v = tags.get(5)
        if strtab_v is None:
            raise NativeFormatError("PT_DYNAMIC has no DT_STRTAB")
        strtab = v2o(strtab_v)
        for tag, val in entries:
            if tag == 1:
                needed.append(_cstr(b, strtab + val))
        vn_v, vn_n = tags.get(0x6FFFFFFE), tags.get(0x6FFFFFFF, 0)
        if vn_v is not None and vn_n:
            off = v2o(vn_v)
            for _ in range(vn_n):
                try:
                    _vn_version, vn_cnt, _vn_file, vn_aux, vn_next = struct.unpack_from("<HHIII", b, off)
                except struct.error as e:
                    raise NativeFormatError(f"truncated verneed: {e}") from None
                aux = off + vn_aux
                for _ in range(vn_cnt):
                    try:
                        _hash, _flags, _other, vna_name, vna_next = struct.unpack_from("<IHHII", b, aux)
                    except struct.error as e:
                        raise NativeFormatError(f"truncated vernaux: {e}") from None
                    name = _cstr(b, strtab + vna_name)
                    version_needs.append(name)
                    m = _GLIBC_VER.match(name)
                    if m:
                        ver = (int(m.group(1)), int(m.group(2)))
                        glibc_max = ver if glibc_max is None or ver > glibc_max else glibc_max
                    if not vna_next:
                        break
                    aux += vna_next
                if not vn_next:
                    break
                off += vn_next
    return NativeInfo(fmt="elf", arch=arch, elf_needed=tuple(needed), elf_version_needs=tuple(version_needs), elf_glibc_max=glibc_max)


def _parse_macho(b: bytes) -> NativeInfo:
    kind, order = _MACHO_MAGICS[b[:4]]
    if kind == "fat":
        return NativeInfo(fmt="macho", arch="fat", macho_fat=True)
    if kind != "macho64":
        raise NativeFormatError("32-bit Mach-O is not a supported release format")
    try:
        cputype = _u32(b, 4, order)
        ncmds = _u32(b, 16, order)
    except struct.error as e:
        raise NativeFormatError(f"truncated Mach-O header: {e}") from None
    arch = {0x01000007: "x86_64", 0x0100000C: "aarch64"}.get(cputype, f"unknown(0x{cputype:08x})")
    minos: tuple[int, int, int] | None = None
    dylibs: list[str] = []
    off = 32  # sizeof(mach_header_64)
    for _ in range(ncmds):
        try:
            cmd, cmdsize = _u32(b, off, order), _u32(b, off + 4, order)
        except struct.error as e:
            raise NativeFormatError(f"truncated load command: {e}") from None
        if cmdsize < 8:
            raise NativeFormatError("load command with cmdsize < 8")
        if cmd in (0x0C, 0x80000018, 0x8000001F):  # LC_LOAD_DYLIB / LC_LOAD_WEAK_DYLIB / LC_REEXPORT_DYLIB
            name_off = _u32(b, off + 8, order)
            dylibs.append(_cstr(b, off + name_off))
        elif cmd == 0x24:  # LC_VERSION_MIN_MACOSX
            v = _u32(b, off + 8, order)
            minos = ((v >> 16) & 0xFFFF, (v >> 8) & 0xFF, v & 0xFF)
        elif cmd == 0x32:  # LC_BUILD_VERSION
            plat, v = _u32(b, off + 8, order), _u32(b, off + 12, order)
            if plat == 1:  # PLATFORM_MACOS
                minos = ((v >> 16) & 0xFFFF, (v >> 8) & 0xFF, v & 0xFF)
        off += cmdsize
    return NativeInfo(fmt="macho", arch=arch, macho_minos=minos, macho_dylibs=tuple(dylibs))


def inspect_binary(data: bytes) -> NativeInfo:
    """Parse the header facts of a native executable. Raises NativeFormatError for anything that
    is not a recognised native image (text shebang wrappers included)."""
    fmt = sniff_format(data[:4])
    if fmt is None:
        head = data[:2]
        why = "a text `#!` wrapper" if head == b"#!" else ("an empty file" if not data else f"unknown magic {data[:4]!r}")
        raise NativeFormatError(f"not a native executable ({why})")
    if fmt == "pe":
        return _parse_pe(data)
    if fmt == "elf":
        return _parse_elf(data)
    return _parse_macho(data)


# ----------------------------------------------------------------------------- tag rules

def tag_requirements(tag: str) -> tuple[str, str, tuple[int, ...] | None]:
    try:
        return _TAG_RULES[tag]
    except KeyError:
        raise ValueError(f"{tag!r} is not in the expected wheel set {sorted(EXPECTED_WHEEL_SET)}") from None


def check_format_and_arch(info: NativeInfo, tag: str) -> list[str]:
    """The cheap half of the floor (format + architecture only). Used at wheel-build time."""
    fmt, arch, _ = tag_requirements(tag)
    problems: list[str] = []
    if info.fmt != fmt:
        problems.append(f"format mismatch: the binary is {info.fmt.upper()}, the tag {tag!r} needs {fmt.upper()}")
    if info.arch != arch:
        problems.append(f"architecture mismatch: the binary is {info.arch}, the tag {tag!r} needs {arch}")
    return problems


def check_floor(info: NativeInfo, tag: str) -> list[str]:
    """The REAL compatibility floor of the binary vs the declared tag (validation §I6). Empty ==
    GREEN. Every finding names both sides so a falsely-permissive tag is diagnosed, not merely
    refused."""
    problems = check_format_and_arch(info, tag)
    fmt, _arch, floor = tag_requirements(tag)
    if problems:
        return problems
    if fmt == "pe":
        if info.pe_subsystem != 3:
            problems.append(f"PE subsystem is {info.pe_subsystem}, the CLI must be a console image (3)")
        if not info.pe_characteristics & 0x0002:
            problems.append("PE characteristics lack IMAGE_FILE_EXECUTABLE_IMAGE")
        if info.pe_characteristics & 0x2000:
            problems.append("PE image is a DLL, not an executable")
    elif fmt == "elf":
        assert floor is not None
        if info.elf_glibc_max is None:
            problems.append("no GLIBC_x.y version requirement found (not a glibc-dynamic executable? the floor cannot be established)")
        elif info.elf_glibc_max > floor:
            have, want = ".".join(map(str, info.elf_glibc_max)), ".".join(map(str, floor))
            problems.append(f"glibc floor too high: the binary needs GLIBC_{have} but {tag!r} promises glibc {want} (falsely-permissive tag; build in the manylinux_2_28 container / cargo-zigbuild --target <triple>.2.28)")
        bad = sorted(set(info.elf_needed) - MANYLINUX_2_28_ALLOWED_LIBS)
        if bad:
            problems.append(f"DT_NEEDED outside the manylinux_2_28 allowlist: {bad}")
    else:
        assert floor is not None
        if info.macho_fat:
            problems.append(f"fat (universal) binary in a single-arch wheel {tag!r}")
        if info.macho_minos is None:
            problems.append("no LC_BUILD_VERSION / LC_VERSION_MIN_MACOSX load command (the deployment floor cannot be established)")
        elif info.macho_minos > floor:
            have, want = ".".join(map(str, info.macho_minos)), ".".join(map(str, floor[:2]))
            problems.append(f"macOS floor too high: the binary's minos is {have} but {tag!r} promises {want} (falsely-permissive tag; build with MACOSX_DEPLOYMENT_TARGET=11.0)")
        bad = [d for d in info.macho_dylibs if not d.startswith(MACOS_SYSTEM_DYLIB_PREFIXES)]
        if bad:
            problems.append(f"dylibs outside the system set {MACOS_SYSTEM_DYLIB_PREFIXES}: {bad}")
    return problems


# ----------------------------------------------------------------------------- host

def _norm_machine(m: str) -> str:
    m = m.lower()
    if m in ("amd64", "x86_64", "x64"):
        return "x86_64"
    if m in ("arm64", "aarch64"):
        return "aarch64"
    return m or "unknown"


def host_expectation() -> tuple[str, str]:
    """(format, normalized arch) a bundled binary must have to run on THIS interpreter's platform."""
    sysname = platform.system()
    fmt = {"Windows": "pe", "Linux": "elf", "Darwin": "macho"}.get(sysname, "unknown")
    return fmt, _norm_machine(platform.machine())


def host_wheel_tag() -> str | None:
    """The expected-set tag for this host, or None when the host is not a release target."""
    fmt, arch = host_expectation()
    for tag, (f, a, _) in _TAG_RULES.items():
        if f == fmt and a == arch:
            return tag
    return None


def host_mismatch(info: NativeInfo) -> str | None:
    """The install-time arch self-check (plan M1.3 'clear error, not SIGILL'): None when the
    bundled binary can run here, else a one-line explanation."""
    fmt, arch = host_expectation()
    if info.fmt != fmt or info.arch != arch:
        return (f"the bundled compiler is a {info.fmt.upper()} {info.arch} executable but this machine is "
                f"{platform.system()} {platform.machine()} -- a pythscribe wheel for another platform was installed "
                "(reinstall with `pip install --force-reinstall pythscribe`, or set PYTHSCRIBE_PYTHS to a native build)")
    return None
