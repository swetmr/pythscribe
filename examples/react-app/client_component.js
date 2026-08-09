"use client";
import { useState } from "react";
export default function ToggleButton() {
    const [active, set_active] = useState(false);
    function toggle() {
        set_active((!active));
    }
    return null;
}
