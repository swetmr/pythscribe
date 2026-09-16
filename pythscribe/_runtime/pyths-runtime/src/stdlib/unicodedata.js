// PythScribe standard library: unicodedata module (subset).
//
// `normalize` maps directly onto JS String.prototype.normalize — the same
// Unicode Normalization Forms (NFC/NFD/NFKC/NFKD). The table-driven API
// surface (name/lookup/category/…) needs the UCD tables and stays
// unimplemented with a loud error (pythscribe-v3.x).

export function normalize(form, s) {
    if (!["NFC", "NFD", "NFKC", "NFKD"].includes(form)) {
        const e = new Error("invalid normalization form");
        e.name = "ValueError";
        throw e;
    }
    return String(s).normalize(form);
}

function __ucdUnsupported(fn) {
    const e = new Error(
        `unicodedata.${fn}() is not supported by PythScribe (requires the ` +
        "Unicode character database tables; planned pythscribe-v3.x). " +
        "Only unicodedata.normalize() is available.",
    );
    e.name = "NotImplementedError";
    throw e;
}
export function name(_c, _default) { __ucdUnsupported("name"); }
export function lookup(_n) { __ucdUnsupported("lookup"); }
export function category(_c) { __ucdUnsupported("category"); }
export function decimal(_c, _d) { __ucdUnsupported("decimal"); }
export function digit(_c, _d) { __ucdUnsupported("digit"); }
export function numeric(_c, _d) { __ucdUnsupported("numeric"); }
export function bidirectional(_c) { __ucdUnsupported("bidirectional"); }
export function combining(_c) { __ucdUnsupported("combining"); }
export function east_asian_width(_c) { __ucdUnsupported("east_asian_width"); }
export function mirrored(_c) { __ucdUnsupported("mirrored"); }
export function decomposition(_c) { __ucdUnsupported("decomposition"); }
export function is_normalized(_form, _s) { __ucdUnsupported("is_normalized"); }
export const unidata_version = "15.0.0";

//# sourceMappingURL=unicodedata.js.map
