// PythScribe stdlib — Python `string` module (the constant character classes;
// the deprecated function helpers are omitted).

export const ascii_lowercase = "abcdefghijklmnopqrstuvwxyz";
export const ascii_uppercase = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
export const ascii_letters = ascii_lowercase + ascii_uppercase;
export const digits = "0123456789";
export const hexdigits = "0123456789abcdefABCDEF";
export const octdigits = "01234567";
export const punctuation = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
export const whitespace = " \t\n\r\x0b\x0c";
export const printable = digits + ascii_letters + punctuation + whitespace;

//# sourceMappingURL=string.js.map
