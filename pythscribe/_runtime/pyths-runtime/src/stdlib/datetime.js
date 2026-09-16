// PythScribe standard library: datetime module
// Maps Python datetime classes to JavaScript Date wrappers

import { __pyF } from "../operators.js"; // Option B float box
import { __pyTypeName } from "../runtime.js"; // #467: the ONE type-name source
export class timedelta {
    constructor({ days = 0, seconds = 0, microseconds = 0, milliseconds = 0, minutes = 0, hours = 0, weeks = 0 } = {}) {
        this._ms = (days * 86400 + hours * 3600 + minutes * 60 + seconds) * 1000 + milliseconds + microseconds / 1000 + weeks * 7 * 86400 * 1000;
    }

    // CPython normalization: 0 <= seconds < 86400, 0 <= microseconds < 1e6,
    // days carries the sign (timedelta(days=-1, seconds=68400) IS the
    // normal form of -5h). JS % is sign-of-dividend, so use a positive mod.
    get days() { return Math.floor(this._ms / 86400000); }
    get seconds() {
        const rem = ((this._ms % 86400000) + 86400000) % 86400000;
        return Math.floor(rem / 1000);
    }
    get microseconds() {
        const rem = ((this._ms % 1000) + 1000) % 1000;
        return Math.round(rem * 1000);
    }

    total_seconds() { return this._ms / 1000; }

    toString() {
        const d = this.days;
        const s = this.seconds;
        const h = Math.floor(s / 3600);
        const m = Math.floor((s % 3600) / 60);
        const sec = s % 60;
        const time = `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
        return d ? `${d} day${Math.abs(d) !== 1 ? "s" : ""}, ${time}` : time;
    }

    // autotester module_datetime: CPython repr — keyword form listing only
    // the nonzero normalized components; all-zero is 'datetime.timedelta(0)'.
    __repr__() {
        const parts = [];
        if (this.days) parts.push(`days=${this.days}`);
        if (this.seconds) parts.push(`seconds=${this.seconds}`);
        if (this.microseconds) parts.push(`microseconds=${this.microseconds}`);
        if (parts.length === 0) return "datetime.timedelta(0)";
        return `datetime.timedelta(${parts.join(", ")})`;
    }

    __eq__(other) {
        return other instanceof timedelta && this._ms === other._ms;
    }

    __lt__(other) {
        if (!(other instanceof timedelta)) {
            const e = new Error("'<' not supported between instances of 'datetime.timedelta' and '" + __pyTypeName(other) + "'"); // #467
            e.name = "TypeError"; throw e;
        }
        return this._ms < other._ms;
    }
}

// autotester module_datetime: fixed-offset timezone (previously missing —
// `from datetime import timezone` was an ImportError-shaped failure).
export class timezone {
    constructor(offset, name) {
        if (!(offset instanceof timedelta)) {
            const e = new Error("offset must be a timedelta");
            e.name = "TypeError";
            throw e;
        }
        this._offset = offset;
        this._name = name;
    }

    utcoffset(_dt) { return this._offset; }

    tzname(_dt) {
        if (this._name !== undefined) return this._name;
        if (this._offset._ms === 0) return "UTC";
        const total = this._offset._ms / 60000; // minutes
        const sign = total < 0 ? "-" : "+";
        const abs = Math.abs(total);
        const h = String(Math.floor(abs / 60)).padStart(2, "0");
        const m = String(Math.round(abs % 60)).padStart(2, "0");
        return `UTC${sign}${h}:${m}`;
    }

    __eq__(other) {
        return other instanceof timezone && this._offset._ms === other._offset._ms
            && this._name === other._name;
    }

    __repr__() {
        if (this === timezone.utc) return "datetime.timezone.utc";
        const off = this._offset.__repr__();
        return this._name === undefined
            ? `datetime.timezone(${off})`
            : `datetime.timezone(${off}, '${this._name}')`;
    }

    toString() { return this.tzname(null); }
}
timezone.utc = new timezone(new timedelta());

export class date {
    constructor(year, month, day) {
        this.year = year;
        this.month = month;
        this.day = day;
    }

    static today() {
        const d = new Date();
        return new date(d.getFullYear(), d.getMonth() + 1, d.getDate());
    }

    static fromisoformat(s) {
        const [y, m, d] = s.split("-").map(Number);
        return new date(y, m, d);
    }

    isoformat() {
        return `${String(this.year).padStart(4, "0")}-${String(this.month).padStart(2, "0")}-${String(this.day).padStart(2, "0")}`;
    }

    weekday() {
        const d = new Date(this.year, this.month - 1, this.day);
        return (d.getDay() + 6) % 7; // Python: Monday=0
    }

    isoweekday() { return this.weekday() + 1; }

    // autotester module_datetime: date.replace(year=…, month=…, day=…) —
    // keyword args arrive as the codegen's trailing kwargs object.
    replace(kw = {}) {
        return new date(kw.year ?? this.year, kw.month ?? this.month, kw.day ?? this.day);
    }

    ctime() {
        return _ctime(this.year, this.month, this.day, 0, 0, 0);
    }

    __repr__() {
        return `datetime.date(${this.year}, ${this.month}, ${this.day})`;
    }

    __lt__(other) {
        if (!(other instanceof date)) {
            const e = new Error("'<' not supported between instances of 'datetime.date' and '" + __pyTypeName(other) + "'"); // #470: same class as #467 — computed name, never a literal "other"
            e.name = "TypeError"; throw e;
        }
        return this.year !== other.year ? this.year < other.year
            : this.month !== other.month ? this.month < other.month
            : this.day < other.day;
    }

    strftime(fmt) {
        return _strftime(fmt, this.year, this.month, this.day, 0, 0, 0);
    }

    __eq__(other) {
        return other instanceof date && this.year === other.year && this.month === other.month && this.day === other.day;
    }

    __sub__(other) {
        if (other instanceof date) {
            const d1 = new Date(this.year, this.month - 1, this.day);
            const d2 = new Date(other.year, other.month - 1, other.day);
            return new timedelta({ milliseconds: d1 - d2 });
        }
        if (other instanceof timedelta) {
            // CPython: date - timedelta subtracts only the normalized DAYS
            // component (`self + timedelta(-other.days)`), so
            // d + delta - delta round-trips even when delta has a time part.
            return this._addDays(-other.days);
        }
    }

    __add__(other) {
        if (other instanceof timedelta) {
            // CPython: only the normalized days component moves a date.
            return this._addDays(other.days);
        }
    }

    _addDays(n) {
        const d = new Date(this.year, this.month - 1, this.day);
        d.setDate(d.getDate() + n);
        return new date(d.getFullYear(), d.getMonth() + 1, d.getDate());
    }

    toString() { return this.isoformat(); }
}

export class time {
    constructor(hour = 0, minute = 0, second = 0, microsecond = 0) {
        this.hour = hour;
        this.minute = minute;
        this.second = second;
        this.microsecond = microsecond;
    }

    isoformat() {
        let s = `${String(this.hour).padStart(2, "0")}:${String(this.minute).padStart(2, "0")}:${String(this.second).padStart(2, "0")}`;
        if (this.microsecond) s += `.${String(this.microsecond).padStart(6, "0")}`;
        return s;
    }

    __repr__() {
        const parts = [this.hour, this.minute];
        if (this.second || this.microsecond) parts.push(this.second);
        if (this.microsecond) parts.push(this.microsecond);
        return `datetime.time(${parts.join(", ")})`;
    }

    toString() { return this.isoformat(); }
}

export class datetime {
    constructor(year, month, day, hour = 0, minute = 0, second = 0, microsecond = 0, tzinfo) {
        // Keyword args arrive as the codegen's trailing kwargs object —
        // `datetime(2010, 1, 1, tzinfo=timezone.utc)` passes {tzinfo} as the
        // 4th positional. Peel a kwargs-shaped trailing argument off
        // whichever positional slot it landed in.
        const args = [hour, minute, second, microsecond];
        let kw = null;
        for (let i = 0; i < args.length; i++) {
            const a = args[i];
            if (a !== null && typeof a === "object" && Object.getPrototypeOf(a) === Object.prototype) {
                kw = a;
                args[i] = [0, 0, 0, 0][i];
                break;
            }
        }
        this.year = year;
        this.month = month;
        this.day = day;
        this.hour = args[0];
        this.minute = args[1];
        this.second = args[2];
        this.microsecond = args[3];
        this.tzinfo = kw && kw.tzinfo !== undefined ? kw.tzinfo : tzinfo;
        if (kw) {
            if (kw.hour !== undefined) this.hour = kw.hour;
            if (kw.minute !== undefined) this.minute = kw.minute;
            if (kw.second !== undefined) this.second = kw.second;
            if (kw.microsecond !== undefined) this.microsecond = kw.microsecond;
        }
    }

    static now(tz) {
        const d = new Date();
        if (tz === undefined || tz === null) {
            return new datetime(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds() * 1000);
        }
        // Aware: current instant expressed in tz (offset minutes from UTC).
        const offMs = tz.utcoffset(null)._ms;
        const t = new Date(d.getTime() + offMs);
        const out = new datetime(t.getUTCFullYear(), t.getUTCMonth() + 1, t.getUTCDate(), t.getUTCHours(), t.getUTCMinutes(), t.getUTCSeconds(), t.getUTCMilliseconds() * 1000);
        out.tzinfo = tz;
        return out;
    }

    // CPython utcnow(): NAIVE datetime carrying the current UTC time.
    static utcnow() {
        const d = new Date();
        return new datetime(d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), d.getUTCMilliseconds() * 1000);
    }

    static today() { return datetime.now(); }

    static fromisoformat(s) {
        const d = new Date(s);
        return new datetime(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds() * 1000);
    }

    static fromtimestamp(ts) {
        const d = new Date(ts * 1000);
        return new datetime(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds() * 1000);
    }

    // #253b: `datetime.strptime(s, fmt)` — the common numeric directives
    // (%Y %m %d %H %M %S %y %f %j %%). Named-month / locale directives are not
    // modeled.
    static strptime(dateString, format) {
        const spec = {
            "%Y": "(\\d{4})", "%m": "(\\d{1,2})", "%d": "(\\d{1,2})",
            "%H": "(\\d{1,2})", "%M": "(\\d{1,2})", "%S": "(\\d{1,2})",
            "%y": "(\\d{2})", "%f": "(\\d{1,6})", "%j": "(\\d{1,3})",
        };
        const order = [];
        let pattern = "";
        for (let i = 0; i < format.length; i++) {
            if (format[i] === "%" && i + 1 < format.length) {
                const d = "%" + format[i + 1];
                if (d === "%%") { pattern += "%"; i++; continue; }
                if (spec[d]) { pattern += spec[d]; order.push(d); i++; continue; }
            }
            pattern += format[i].replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        }
        const m = new RegExp("^" + pattern + "$").exec(dateString);
        if (!m) {
            const e = new Error(`time data '${dateString}' does not match format '${format}'`);
            e.name = "ValueError";
            throw e;
        }
        const f = { year: 1900, month: 1, day: 1, hour: 0, minute: 0, second: 0, microsecond: 0 };
        order.forEach((d, idx) => {
            const v = parseInt(m[idx + 1], 10);
            if (d === "%Y") f.year = v;
            else if (d === "%y") f.year = 2000 + v;
            else if (d === "%m") f.month = v;
            else if (d === "%d") f.day = v;
            else if (d === "%H") f.hour = v;
            else if (d === "%M") f.minute = v;
            else if (d === "%S") f.second = v;
            else if (d === "%f") f.microsecond = v;
        });
        return new datetime(f.year, f.month, f.day, f.hour, f.minute, f.second, f.microsecond);
    }

    isoformat(sep = "T") {
        let s = `${String(this.year).padStart(4, "0")}-${String(this.month).padStart(2, "0")}-${String(this.day).padStart(2, "0")}`;
        s += `${sep}${String(this.hour).padStart(2, "0")}:${String(this.minute).padStart(2, "0")}:${String(this.second).padStart(2, "0")}`;
        if (this.microsecond) s += `.${String(this.microsecond).padStart(6, "0")}`;
        if (this.tzinfo) s += _offsetStr(this.tzinfo.utcoffset(this)._ms);
        return s;
    }

    // autotester module_datetime: replace(year=…, …, tzinfo=…) — keyword
    // args arrive as the codegen's trailing kwargs object.
    replace(kw = {}) {
        const out = new datetime(
            kw.year ?? this.year, kw.month ?? this.month, kw.day ?? this.day,
            kw.hour ?? this.hour, kw.minute ?? this.minute, kw.second ?? this.second,
            kw.microsecond ?? this.microsecond,
        );
        out.tzinfo = Object.prototype.hasOwnProperty.call(kw, "tzinfo") ? kw.tzinfo : this.tzinfo;
        return out;
    }

    // astimezone(tz=None): convert to tz (None → the LOCAL timezone). A
    // naive datetime is assumed to be local time (CPython semantics).
    astimezone(tz) {
        if (tz !== null && typeof tz === "object" && Object.getPrototypeOf(tz) === Object.prototype) {
            tz = tz.tz; // kwargs form: astimezone(tz=None)
        }
        // Epoch instant of this datetime.
        let epoch;
        if (this.tzinfo) {
            epoch = Date.UTC(this.year, this.month - 1, this.day, this.hour, this.minute, this.second, this.microsecond / 1000)
                - this.tzinfo.utcoffset(this)._ms;
        } else {
            epoch = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second, this.microsecond / 1000).getTime();
        }
        if (tz === undefined || tz === null) {
            const d = new Date(epoch);
            const out = new datetime(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds() * 1000);
            out.tzinfo = new timezone(new timedelta({ minutes: -d.getTimezoneOffset() }));
            return out;
        }
        const offMs = tz.utcoffset(null)._ms;
        const t = new Date(epoch + offMs);
        const out = new datetime(t.getUTCFullYear(), t.getUTCMonth() + 1, t.getUTCDate(), t.getUTCHours(), t.getUTCMinutes(), t.getUTCSeconds(), t.getUTCMilliseconds() * 1000);
        out.tzinfo = tz;
        return out;
    }

    ctime() {
        return _ctime(this.year, this.month, this.day, this.hour, this.minute, this.second);
    }

    // tuple(d.timetuple()) → (year, mon, mday, hour, min, sec, wday, yday, isdst)
    timetuple() {
        return _timetuple(this.year, this.month, this.day, this.hour, this.minute, this.second, -1);
    }

    utctimetuple() {
        // Naive: identical fields with isdst=0; aware: convert to UTC first.
        if (this.tzinfo) {
            const epoch = Date.UTC(this.year, this.month - 1, this.day, this.hour, this.minute, this.second)
                - this.tzinfo.utcoffset(this)._ms;
            const t = new Date(epoch);
            return _timetuple(t.getUTCFullYear(), t.getUTCMonth() + 1, t.getUTCDate(), t.getUTCHours(), t.getUTCMinutes(), t.getUTCSeconds(), 0);
        }
        return _timetuple(this.year, this.month, this.day, this.hour, this.minute, this.second, 0);
    }

    __repr__() {
        const parts = [this.year, this.month, this.day, this.hour, this.minute];
        if (this.second || this.microsecond) parts.push(this.second);
        if (this.microsecond) parts.push(this.microsecond);
        let s = `datetime.datetime(${parts.join(", ")}`;
        if (this.tzinfo) s += `, tzinfo=${this.tzinfo.__repr__()}`;
        return s + ")";
    }

    __lt__(other) {
        if (!(other instanceof datetime)) {
            const e = new Error("'<' not supported between instances of 'datetime.datetime' and '" + __pyTypeName(other) + "'"); // #470: same class as #467 — computed name, never a literal "other"
            e.name = "TypeError"; throw e;
        }
        return this._cmpKey() < other._cmpKey();
    }

    _cmpKey() {
        // µs-exact (Date.UTC would truncate a fractional-ms argument).
        const offUs = this.tzinfo ? this.tzinfo.utcoffset(this)._ms * 1000 : 0;
        return Date.UTC(this.year, this.month - 1, this.day, this.hour, this.minute, this.second) * 1000
            + this.microsecond - offUs;
    }

    timestamp() {
        const d = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second, this.microsecond / 1000);
        // Option B: CPython datetime.timestamp() is a float — a whole-second
        // timestamp (microsecond == 0) must keep the float tag.
        return __pyF(d.getTime() / 1000);
    }

    date() { return new date(this.year, this.month, this.day); }
    time() { return new time(this.hour, this.minute, this.second, this.microsecond); }

    weekday() {
        const d = new Date(this.year, this.month - 1, this.day);
        return (d.getDay() + 6) % 7;
    }

    strftime(fmt) {
        return _strftime(fmt, this.year, this.month, this.day, this.hour, this.minute, this.second);
    }

    __eq__(other) {
        return other instanceof datetime && this._cmpKey() === other._cmpKey();
    }

    // µs-exact arithmetic: the old Date-millisecond round trip truncated
    // microseconds, so d + delta - delta drifted (CPython is exact).
    _epochUs() {
        return Date.UTC(this.year, this.month - 1, this.day, this.hour, this.minute, this.second) * 1000 + this.microsecond;
    }

    static _fromEpochUs(us, tzinfo) {
        const usInSec = ((us % 1e6) + 1e6) % 1e6;
        const d = new Date((us - usInSec) / 1000);
        const out = new datetime(d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), usInSec);
        out.tzinfo = tzinfo;
        return out;
    }

    __sub__(other) {
        if (other instanceof datetime) {
            return new timedelta({ microseconds: this._cmpKey() - other._cmpKey() });
        }
        if (other instanceof timedelta) {
            return datetime._fromEpochUs(this._epochUs() - Math.round(other._ms * 1000), this.tzinfo);
        }
    }

    __add__(other) {
        if (other instanceof timedelta) {
            return datetime._fromEpochUs(this._epochUs() + Math.round(other._ms * 1000), this.tzinfo);
        }
    }

    toString() { return this.isoformat(" "); }
}

// CPython ctime(): 'Sat Jan  1 00:00:00 2016' — day space-padded to 2.
function _ctime(year, month, day, hour, minute, second) {
    const dayNames = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const monthNames = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const d = new Date(year, month - 1, day);
    const wd = (d.getDay() + 6) % 7;
    const t = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}:${String(second).padStart(2, "0")}`;
    return `${dayNames[wd]} ${monthNames[month - 1]} ${String(day).padStart(2, " ")} ${t} ${year}`;
}

// tuple(struct_time): (year, mon, mday, hour, min, sec, wday, yday, isdst).
function _timetuple(year, month, day, hour, minute, second, isdst) {
    const d = new Date(year, month - 1, day);
    const wday = (d.getDay() + 6) % 7;
    const yday = Math.floor((d - new Date(year, 0, 0)) / 86400000);
    const items = [year, month, day, hour, minute, second, wday, yday, isdst];
    Object.defineProperty(items, "__pytuple__", { value: true, enumerable: false });
    return items;
}

// '+00:00' / '-05:00' isoformat offset suffix from an offset in ms.
function _offsetStr(offMs) {
    const total = offMs / 60000;
    const sign = total < 0 ? "-" : "+";
    const abs = Math.abs(total);
    return `${sign}${String(Math.floor(abs / 60)).padStart(2, "0")}:${String(Math.round(abs % 60)).padStart(2, "0")}`;
}

function _strftime(fmt, year, month, day, hour, minute, second) {
    const dayNames = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const monthNames = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const d = new Date(year, month - 1, day);
    const wd = (d.getDay() + 6) % 7;

    return fmt.replace(/%[a-zA-Z%]/g, (match) => {
        switch (match) {
            case "%Y": return String(year).padStart(4, "0");
            case "%m": return String(month).padStart(2, "0");
            case "%d": return String(day).padStart(2, "0");
            case "%H": return String(hour).padStart(2, "0");
            case "%M": return String(minute).padStart(2, "0");
            case "%S": return String(second).padStart(2, "0");
            case "%A": return dayNames[wd];
            case "%a": return dayNames[wd].slice(0, 3);
            case "%B": return monthNames[month - 1];
            case "%b": return monthNames[month - 1].slice(0, 3);
            case "%w": return String((wd + 1) % 7);
            case "%j": {
                const start = new Date(year, 0, 0);
                const diff = d - start;
                return String(Math.floor(diff / 86400000)).padStart(3, "0");
            }
            case "%I": return String(hour % 12 || 12).padStart(2, "0");
            case "%p": return hour < 12 ? "AM" : "PM";
            case "%%": return "%";
            default: return match;
        }
    });
}

//# sourceMappingURL=datetime.js.map
