import { c as e, i as t, l as n, n as r, o as i, s as a, t as o } from "./client-JrhCN0ye.js";
//#region frontend/Example.svelte
var s = a("<div> </div>");
function c(a, c) {
	let l = o(c, "selected", 3, !1);
	var u = s();
	let d;
	var f = n(u, !0);
	e(() => {
		d = r(u, 1, "svelte-s3apn9", null, d, {
			table: c.type === "table",
			gallery: c.type === "gallery",
			selected: l()
		}), t(f, c.value);
	}), i(a, u);
}
//#endregion
export { c as default };
