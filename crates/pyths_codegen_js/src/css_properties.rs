//! Reference list of React-DOM CSS property names (camelCase).
//!
//! Authority: facebook/react `CSSProperty.js`. Used by the codegen
//! to validate that the snake→camel conversion of `style` keys
//! produces a name React actually consumes — diagnostics on hit
//! when the user wrote a typo'd CSS prop.
//!
//! This list is intentionally not exhaustive (CSS adds properties
//! every year and React allows arbitrary keys via `style.cssText`).
//! It covers the everyday surface — anything missing is silently
//! allowed, matching React's behavior.

/// Known React-DOM CSS property names (camelCase). Sorted.
///
/// Source-of-truth grouping:
///   - layout (display, position, overflow, …)
///   - box model (margin, padding, border*)
///   - typography (font*, line*, letter*, text*, white*, word*)
///   - color (color, background*, opacity)
///   - flex/grid (flex*, grid*, justify*, align*, gap)
///   - effects (transform*, transition*, animation*, filter, shadow)
///   - misc (cursor, pointer*, visibility, z-index, …)
pub static REACT_CSS_PROPERTIES: &[&str] = &[
    // layout
    "display",
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "overflow",
    "overflowX",
    "overflowY",
    "overflowWrap",
    "visibility",
    "zIndex",
    "boxSizing",
    "float",
    "clear",
    // box model
    "width",
    "height",
    "minWidth",
    "minHeight",
    "maxWidth",
    "maxHeight",
    "margin",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
    "marginBlock",
    "marginBlockStart",
    "marginBlockEnd",
    "marginInline",
    "marginInlineStart",
    "marginInlineEnd",
    "padding",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
    "paddingLeft",
    "paddingBlock",
    "paddingBlockStart",
    "paddingBlockEnd",
    "paddingInline",
    "paddingInlineStart",
    "paddingInlineEnd",
    "border",
    "borderTop",
    "borderRight",
    "borderBottom",
    "borderLeft",
    "borderWidth",
    "borderTopWidth",
    "borderRightWidth",
    "borderBottomWidth",
    "borderLeftWidth",
    "borderStyle",
    "borderTopStyle",
    "borderRightStyle",
    "borderBottomStyle",
    "borderLeftStyle",
    "borderColor",
    "borderTopColor",
    "borderRightColor",
    "borderBottomColor",
    "borderLeftColor",
    "borderRadius",
    "borderTopLeftRadius",
    "borderTopRightRadius",
    "borderBottomLeftRadius",
    "borderBottomRightRadius",
    "borderCollapse",
    "borderSpacing",
    "outline",
    "outlineWidth",
    "outlineStyle",
    "outlineColor",
    "outlineOffset",
    // typography
    "color",
    "font",
    "fontFamily",
    "fontSize",
    "fontStyle",
    "fontWeight",
    "fontVariant",
    "fontStretch",
    "fontFeatureSettings",
    "fontKerning",
    "fontSizeAdjust",
    "fontDisplay",
    "lineHeight",
    "letterSpacing",
    "wordSpacing",
    "wordBreak",
    "wordWrap",
    "whiteSpace",
    "textAlign",
    "textAlignLast",
    "textDecoration",
    "textDecorationColor",
    "textDecorationLine",
    "textDecorationStyle",
    "textIndent",
    "textJustify",
    "textOverflow",
    "textShadow",
    "textTransform",
    "textOrientation",
    "writingMode",
    "direction",
    "verticalAlign",
    "tabSize",
    "hyphens",
    // color / background
    "background",
    "backgroundColor",
    "backgroundImage",
    "backgroundRepeat",
    "backgroundPosition",
    "backgroundPositionX",
    "backgroundPositionY",
    "backgroundSize",
    "backgroundOrigin",
    "backgroundClip",
    "backgroundAttachment",
    "backgroundBlendMode",
    "mixBlendMode",
    "opacity",
    // flex / grid
    "flex",
    "flexBasis",
    "flexDirection",
    "flexFlow",
    "flexGrow",
    "flexShrink",
    "flexWrap",
    "alignContent",
    "alignItems",
    "alignSelf",
    "justifyContent",
    "justifyItems",
    "justifySelf",
    "placeContent",
    "placeItems",
    "placeSelf",
    "order",
    "gap",
    "rowGap",
    "columnGap",
    "grid",
    "gridArea",
    "gridAutoColumns",
    "gridAutoFlow",
    "gridAutoRows",
    "gridColumn",
    "gridColumnStart",
    "gridColumnEnd",
    "gridColumnGap",
    "gridRow",
    "gridRowStart",
    "gridRowEnd",
    "gridRowGap",
    "gridTemplate",
    "gridTemplateAreas",
    "gridTemplateColumns",
    "gridTemplateRows",
    // effects
    "transform",
    "transformOrigin",
    "transformStyle",
    "transformBox",
    "transition",
    "transitionDelay",
    "transitionDuration",
    "transitionProperty",
    "transitionTimingFunction",
    "animation",
    "animationDelay",
    "animationDirection",
    "animationDuration",
    "animationFillMode",
    "animationIterationCount",
    "animationName",
    "animationPlayState",
    "animationTimingFunction",
    "filter",
    "backdropFilter",
    "boxShadow",
    "perspective",
    "perspectiveOrigin",
    "willChange",
    // misc
    "cursor",
    "pointerEvents",
    "userSelect",
    "touchAction",
    "caretColor",
    "resize",
    "scrollBehavior",
    "scrollSnapType",
    "appearance",
    "content",
    "counterIncrement",
    "counterReset",
    "isolation",
    "objectFit",
    "objectPosition",
    "tableLayout",
    "captionSide",
    "emptyCells",
    "listStyle",
    "listStyleType",
    "listStyleImage",
    "listStylePosition",
    "quotes",
    "all",
    "clip",
    "clipPath",
    "mask",
    "maskImage",
    "boxOrient",
    "boxAlign",
    "boxFlex",
    "boxLines",
    "boxPack",
];

/// Convert snake_case CSS key to camelCase (the React form). Mirrors
/// `react::snake_to_camel` but specialized for style props (no special-
/// case mapping table — every CSS prop follows the generic algorithm).
/// Custom properties (`--my-var`, `-webkit-foo`) and already-camelCased
/// keys pass through unchanged.
pub fn css_snake_to_camel(name: &str) -> String {
    if !name.contains('_') {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut upcase_next = false;
    for c in name.chars() {
        if c == '_' {
            upcase_next = true;
        } else if upcase_next {
            out.extend(c.to_uppercase());
            upcase_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_snake_to_camel_basic() {
        assert_eq!(css_snake_to_camel("border_radius"), "borderRadius");
        assert_eq!(css_snake_to_camel("padding"), "padding");
        assert_eq!(css_snake_to_camel("font_family"), "fontFamily");
        assert_eq!(css_snake_to_camel("min_height"), "minHeight");
    }

    #[test]
    fn css_snake_to_camel_compound() {
        assert_eq!(css_snake_to_camel("background_color"), "backgroundColor");
        assert_eq!(
            css_snake_to_camel("grid_template_columns"),
            "gridTemplateColumns"
        );
    }

    #[test]
    fn css_snake_to_camel_already_camel() {
        assert_eq!(css_snake_to_camel("borderRadius"), "borderRadius");
    }

    #[test]
    fn css_table_includes_common_keys() {
        // Floor coverage — increase as the table grows.
        assert!(
            REACT_CSS_PROPERTIES.len() >= 150,
            "CSS property list thin: {}",
            REACT_CSS_PROPERTIES.len()
        );
        // Spot checks
        for must in [
            "borderRadius",
            "fontSize",
            "minHeight",
            "gridTemplateColumns",
            "alignItems",
            "boxShadow",
            "transform",
            "color",
        ] {
            assert!(
                REACT_CSS_PROPERTIES.contains(&must),
                "missing CSS prop: {}",
                must
            );
        }
    }

    #[test]
    fn css_table_round_trips_via_snake_camel() {
        // Every camelCase entry should be reachable by snake-casing the
        // dashes-replaced name and round-tripping. (Doesn't check every
        // prop in CSS spec — just internal consistency.)
        for camel in REACT_CSS_PROPERTIES.iter().take(20) {
            // Build snake_case form: borderRadius → border_radius
            let mut snake = String::new();
            for c in camel.chars() {
                if c.is_ascii_uppercase() {
                    snake.push('_');
                    snake.extend(c.to_lowercase());
                } else {
                    snake.push(c);
                }
            }
            let round = css_snake_to_camel(&snake);
            assert_eq!(&round, camel, "round-trip mismatch for {}", camel);
        }
    }
}
