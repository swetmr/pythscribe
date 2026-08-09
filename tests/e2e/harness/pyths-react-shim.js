// Bridge `pyths-runtime/react` to React's hooks + a passthrough
// `component` decorator.
//
// The codegen emits:
//   import { component, useState, useEffect, useMemo, useCallback } from "pyths-runtime/react";
//
// `component` is metadata-only at the JS layer (the compiler already
// strips the decorator's call). The hooks need to resolve to React.

import * as React from "react";

export function component(fn) { return fn; }
// `@psx` is a codegen-meta decorator that enables PSX-mode emission
// without imposing @component's named-export + props-destructure
// semantics. Identity passthrough at runtime; consumed by codegen.
export function psx(fn) { return fn; }

// Both casings — the codegen camelCases hook imports for `pyths.react`,
// but the snake_case names are kept too in case any path still emits them.
export const useState = React.useState;
export const useEffect = React.useEffect;
export const useMemo = React.useMemo;
export const useCallback = React.useCallback;
export const useRef = React.useRef;
export const useContext = React.useContext;
export const useReducer = React.useReducer;
export const use_state = React.useState;
export const use_effect = React.useEffect;
export const use_memo = React.useMemo;
export const use_callback = React.useCallback;
export const use_ref = React.useRef;
export const use_context = React.useContext;
export const use_reducer = React.useReducer;
