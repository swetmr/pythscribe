// PythScribe standard library: datetime module
// Maps Python datetime classes to JavaScript Date wrappers

export class timedelta {
    constructor({ days = 0, seconds = 0, microseconds = 0, milliseconds = 0, minutes = 0, hours = 0, weeks = 0 } = {}) {
        this._ms = (days * 86400 + hours * 3600 + minutes * 60 + seconds) * 1000 + milliseconds + microseconds / 1000 + weeks * 7 * 86400 * 1000;
    }

    get days() { return Math.floor(this._ms / 86400000); }
    get seconds() { return Math.floor((this._ms % 86400000) / 1000); }
    get microseconds() { return Math.round((this._ms % 1000) * 1000); }

    total_seconds() { return this._ms / 1000; }

    toString() {
        const d = this.days;
        const s = this.seconds;
        const h = Math.floor(s / 3600);
        const m = Math.floor((s % 3600) / 60);
        const sec = s % 60;
        const time = `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
        return d ? `${d} day${d !== 1 ? "s" : ""}, ${time}` : time;
    }
}

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
            const d = new Date(this.year, this.month - 1, this.day);
            d.setMilliseconds(d.getMilliseconds() - other._ms);
            return new date(d.getFullYear(), d.getMonth() + 1, d.getDate());
        }
    }

    __add__(other) {
        if (other instanceof timedelta) {
            const d = new Date(this.year, this.month - 1, this.day);
            d.setMilliseconds(d.getMilliseconds() + other._ms);
            return new date(d.getFullYear(), d.getMonth() + 1, d.getDate());
        }
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

    toString() { return this.isoformat(); }
}

export class datetime {
    constructor(year, month, day, hour = 0, minute = 0, second = 0, microsecond = 0) {
        this.year = year;
        this.month = month;
        this.day = day;
        this.hour = hour;
        this.minute = minute;
        this.second = second;
        this.microsecond = microsecond;
    }

    static now() {
        const d = new Date();
        return new datetime(d.getFullYear(), d.getMonth() + 1, d.getDate(), d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds() * 1000);
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
        return s;
    }

    timestamp() {
        const d = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second, this.microsecond / 1000);
        return d.getTime() / 1000;
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
        return other instanceof datetime && this.year === other.year && this.month === other.month && this.day === other.day && this.hour === other.hour && this.minute === other.minute && this.second === other.second;
    }

    __sub__(other) {
        if (other instanceof datetime) {
            const d1 = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second);
            const d2 = new Date(other.year, other.month - 1, other.day, other.hour, other.minute, other.second);
            return new timedelta({ milliseconds: d1 - d2 });
        }
        if (other instanceof timedelta) {
            const d = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second);
            d.setMilliseconds(d.getMilliseconds() - other._ms);
            return datetime.fromtimestamp(d.getTime() / 1000);
        }
    }

    __add__(other) {
        if (other instanceof timedelta) {
            const d = new Date(this.year, this.month - 1, this.day, this.hour, this.minute, this.second);
            d.setMilliseconds(d.getMilliseconds() + other._ms);
            return datetime.fromtimestamp(d.getTime() / 1000);
        }
    }

    toString() { return this.isoformat(" "); }
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
