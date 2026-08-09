"use client";
import { createElement } from "react";
export default function Profile() {
    return createElement("div", {className: "profile"}, createElement("img", {src: "/avatar.png", alt: "Avatar"}), createElement("hr", null), createElement("br", null), createElement("input", {type: "text", placeholder: "Name", autoFocus: true}));
}
