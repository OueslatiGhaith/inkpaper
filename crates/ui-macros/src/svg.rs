use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use roxmltree::{Document, Node};
use svgtypes::{Length, LengthUnit, NumberListParser, Paint, PathParser, PathSegment};
use syn::LitStr;

const ELLIPSE_KAPPA: f64 = 0.552_284_749_830_793_6;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParsedPaint {
    Color(u8, u8, u8),
    CurrentColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedStrokeCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedStrokeJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ParsedStyle {
    fill: Option<ParsedPaint>,
    fill_rule: ParsedFillRule,
    stroke: Option<ParsedPaint>,
    stroke_width: f32,
    stroke_cap: ParsedStrokeCap,
    stroke_join: ParsedStrokeJoin,
    stroke_miter_limit: f32,
}

impl Default for ParsedStyle {
    fn default() -> Self {
        Self {
            // SVG's initial fill value is black.
            fill: Some(ParsedPaint::Color(0, 0, 0)),
            fill_rule: ParsedFillRule::NonZero,
            stroke: None,
            stroke_width: 1.0,
            stroke_cap: ParsedStrokeCap::Butt,
            stroke_join: ParsedStrokeJoin::Miter,
            stroke_miter_limit: 4.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParsedCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadraticTo {
        control: (f32, f32),
        to: (f32, f32),
    },
    CubicTo {
        control_1: (f32, f32),
        control_2: (f32, f32),
        to: (f32, f32),
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ParsedFill {
    paint: ParsedPaint,
    rule: ParsedFillRule,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ParsedStroke {
    paint: ParsedPaint,
    width: f32,
    cap: ParsedStrokeCap,
    join: ParsedStrokeJoin,
    miter_limit: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedPath {
    commands: Vec<ParsedCommand>,
    fill: Option<ParsedFill>,
    stroke: Option<ParsedStroke>,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedSvg {
    width: i32,
    height: i32,
    view_box: [f32; 4],
    paths: Vec<ParsedPath>,
}

pub(crate) fn expand_include_svg(path: LitStr) -> syn::Result<TokenStream> {
    let resolved = resolve_include_path(&path)?;

    let source = fs::read_to_string(&resolved).map_err(|error| {
        syn::Error::new(
            path.span(),
            format!("could not read SVG `{}`: {error}", resolved.display(),),
        )
    })?;

    let parsed = parse_svg(&source).map_err(|error| {
        syn::Error::new(
            path.span(),
            format!("could not parse SVG `{}`: {error}", resolved.display(),),
        )
    })?;

    let dependency_path = resolved.to_string_lossy();
    let dependency_path = LitStr::new(&dependency_path, path.span());

    Ok(emit_svg(parsed, dependency_path))
}

fn resolve_include_path(path: &LitStr) -> syn::Result<PathBuf> {
    let requested = PathBuf::from(path.value());

    if requested.is_absolute() {
        return Ok(requested);
    }

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        syn::Error::new(
            path.span(),
            "`CARGO_MANIFEST_DIR` is unavailable while expanding `include_svg!`",
        )
    })?;

    Ok(Path::new(&manifest_dir).join(requested))
}

fn parse_svg(source: &str) -> Result<ParsedSvg, String> {
    let document = Document::parse(source).map_err(|error| format!("invalid XML: {error}"))?;

    let root = document.root_element();

    if root.tag_name().name() != "svg" {
        return Err("document root must be <svg>".into());
    }

    validate_preserve_aspect_ratio(root)?;

    let (width, height, view_box) = parse_root_geometry(root)?;

    let root_style = resolve_style(root, ParsedStyle::default())?;

    let mut paths = Vec::new();

    parse_children(root, root_style, &mut paths)?;

    Ok(ParsedSvg {
        width,
        height,
        view_box,
        paths,
    })
}

fn parse_root_geometry(root: Node<'_, '_>) -> Result<(i32, i32, [f32; 4]), String> {
    let explicit_width = root
        .attribute("width")
        .map(|value| parse_length(value, "SVG width"))
        .transpose()?;

    let explicit_height = root
        .attribute("height")
        .map(|value| parse_length(value, "SVG height"))
        .transpose()?;

    if explicit_width.is_some_and(|width| width <= 0.0) {
        return Err("SVG width must be positive".into());
    }

    if explicit_height.is_some_and(|height| height <= 0.0) {
        return Err("SVG height must be positive".into());
    }

    let view_box = match root.attribute("viewBox") {
        Some(value) => parse_view_box(value)?,

        None => {
            let (Some(width), Some(height)) = (explicit_width, explicit_height) else {
                return Err("<svg> requires a `viewBox`, or both `width` and `height`".into());
            };

            [
                0.0,
                0.0,
                checked_f32(width, "SVG width")?,
                checked_f32(height, "SVG height")?,
            ]
        }
    };

    let view_width = view_box[2] as f64;
    let view_height = view_box[3] as f64;

    let (width, height) = match (explicit_width, explicit_height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, width * view_height / view_width),
        (None, Some(height)) => (height * view_width / view_height, height),
        (None, None) => (view_width, view_height),
    };

    Ok((
        intrinsic_pixel_size(width, "SVG width")?,
        intrinsic_pixel_size(height, "SVG height")?,
        view_box,
    ))
}

fn parse_view_box(value: &str) -> Result<[f32; 4], String> {
    let numbers = NumberListParser::from(value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("invalid viewBox `{value}`: {error}"))?;

    let [min_x, min_y, width, height] = numbers.as_slice() else {
        return Err(format!(
            "viewBox must contain exactly four numbers, got {}",
            numbers.len(),
        ));
    };

    if *width <= 0.0 || *height <= 0.0 {
        return Err("viewBox width and height must be positive".into());
    }

    Ok([
        checked_f32(*min_x, "viewBox min-x")?,
        checked_f32(*min_y, "viewBox min-y")?,
        checked_f32(*width, "viewBox width")?,
        checked_f32(*height, "viewBox height")?,
    ])
}

fn validate_preserve_aspect_ratio(root: Node<'_, '_>) -> Result<(), String> {
    let Some(value) = root.attribute("preserveAspectRatio") else {
        return Ok(());
    };

    let values = value.split_whitespace().collect::<Vec<_>>();

    match values.as_slice() {
        ["xMidYMid"] | ["xMidYMid", "meet"] => Ok(()),
        _ => Err(format!(
            "preserveAspectRatio `{value}` is not supported yet"
        )),
    }
}

fn parse_children(
    parent: Node<'_, '_>,
    inherited_style: ParsedStyle,
    paths: &mut Vec<ParsedPath>,
) -> Result<(), String> {
    for node in parent.children().filter(|child| child.is_element()) {
        let name = node.tag_name().name();

        match name {
            "title" | "desc" | "metadata" => {}

            // definitions don't paint by themselves.
            // references into them are rejected when encountered.
            "defs" => {}

            "g" => {
                let style = resolve_style(node, inherited_style)?;

                parse_children(node, style, paths)?;
            }

            "path" | "line" | "polyline" | "polygon" | "rect" | "circle" | "ellipse" => {
                let style = resolve_style(node, inherited_style)?;

                let commands = parse_shape(node)?;

                push_path(paths, commands, style);
            }

            "style" => {
                return Err("<style> blocks are not supported by include_svg!; \
                     use presentation attributes or inline style declarations"
                    .into());
            }

            "svg" => {
                return Err("nested <svg> elements are not supported yet".into());
            }

            other => {
                return Err(format!("unsupported SVG element <{other}>"));
            }
        }
    }

    Ok(())
}

fn push_path(paths: &mut Vec<ParsedPath>, commands: Vec<ParsedCommand>, style: ParsedStyle) {
    if commands.is_empty() {
        return;
    }

    let fill = style.fill.map(|paint| ParsedFill {
        paint,
        rule: style.fill_rule,
    });

    let stroke = match style.stroke {
        Some(paint) if style.stroke_width > 0.0 => Some(ParsedStroke {
            paint,
            width: style.stroke_width,
            cap: style.stroke_cap,
            join: style.stroke_join,
            miter_limit: style.stroke_miter_limit,
        }),

        _ => None,
    };

    if fill.is_none() && stroke.is_none() {
        return;
    }

    paths.push(ParsedPath {
        commands,
        fill,
        stroke,
    });
}

fn resolve_style(node: Node<'_, '_>, inherited: ParsedStyle) -> Result<ParsedStyle, String> {
    validate_non_style_features(node)?;

    let mut style = inherited;

    for property in [
        "fill",
        "fill-rule",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "stroke-miterlimit",
        "opacity",
        "fill-opacity",
        "stroke-opacity",
        "stroke-dasharray",
        "stroke-dashoffset",
        "paint-order",
        "vector-effect",
        "display",
        "visibility",
        "color",
    ] {
        if let Some(value) = node.attribute(property) {
            apply_style_property(&mut style, inherited, property, value)?;
        }
    }

    if let Some(inline_style) = node.attribute("style") {
        for declaration in inline_style.split(';') {
            let declaration = declaration.trim();

            if declaration.is_empty() {
                continue;
            }

            let Some((property, value)) = declaration.split_once(':') else {
                return Err(format!(
                    "invalid inline SVG style declaration `{declaration}`"
                ));
            };

            apply_style_property(&mut style, inherited, property.trim(), value.trim())?;
        }
    }

    Ok(style)
}

fn validate_non_style_features(node: Node<'_, '_>) -> Result<(), String> {
    if let Some(class) = node.attribute("class")
        && !class.trim().is_empty()
    {
        return Err(format!(
            "SVG CSS classes are not supported yet (`class=\"{class}\"`)"
        ));
    }

    if let Some(transform) = node.attribute("transform")
        && !transform.trim().is_empty()
    {
        return Err(format!(
            "SVG transforms are not supported yet (`transform=\"{transform}\"`)"
        ));
    }

    for property in [
        "clip-path",
        "mask",
        "filter",
        "marker-start",
        "marker-mid",
        "marker-end",
    ] {
        if let Some(value) = node.attribute(property)
            && value.trim() != "none"
        {
            return Err(format!("SVG `{property}` is not supported yet"));
        }
    }

    Ok(())
}

fn apply_style_property(
    style: &mut ParsedStyle,
    inherited: ParsedStyle,
    property: &str,
    value: &str,
) -> Result<(), String> {
    let value = value.trim();

    match property {
        "fill" => {
            style.fill = parse_paint(value, inherited.fill)?;
        }

        "fill-rule" => {
            style.fill_rule = match value {
                "nonzero" => ParsedFillRule::NonZero,
                "evenodd" => ParsedFillRule::EvenOdd,
                "inherit" => inherited.fill_rule,

                _ => {
                    return Err(format!("unsupported fill-rule `{value}`"));
                }
            };
        }

        "stroke" => {
            style.stroke = parse_paint(value, inherited.stroke)?;
        }

        "stroke-width" => {
            if value == "inherit" {
                style.stroke_width = inherited.stroke_width;
            } else {
                let width = parse_length(value, "stroke-width")?;

                if width < 0.0 {
                    return Err("stroke-width must not be negative".into());
                }

                style.stroke_width = checked_f32(width, "stroke-width")?;
            }
        }

        "stroke-linecap" => {
            style.stroke_cap = match value {
                "butt" => ParsedStrokeCap::Butt,
                "round" => ParsedStrokeCap::Round,
                "square" => ParsedStrokeCap::Square,
                "inherit" => inherited.stroke_cap,

                _ => {
                    return Err(format!("unsupported stroke-linecap `{value}`"));
                }
            };
        }

        "stroke-linejoin" => {
            style.stroke_join = match value {
                "miter" => ParsedStrokeJoin::Miter,
                "round" => ParsedStrokeJoin::Round,
                "bevel" => ParsedStrokeJoin::Bevel,
                "inherit" => inherited.stroke_join,

                _ => {
                    return Err(format!("unsupported stroke-linejoin `{value}`"));
                }
            };
        }

        "stroke-miterlimit" => {
            if value == "inherit" {
                style.stroke_miter_limit = inherited.stroke_miter_limit;
            } else {
                let limit = value
                    .parse::<f32>()
                    .map_err(|_| format!("invalid stroke-miterlimit `{value}`"))?;

                if !limit.is_finite() || limit < 1.0 {
                    return Err("stroke-miterlimit must be a finite value >= 1".into());
                }

                style.stroke_miter_limit = limit;
            }
        }

        "opacity" | "fill-opacity" | "stroke-opacity" => {
            if value != "inherit" && !is_one(value) {
                return Err(format!(
                    "`{property}={value}` is not supported yet because InkPaper colors do not carry alpha"
                ));
            }
        }

        "stroke-dasharray" => {
            if value != "none" && value != "inherit" {
                return Err("stroke-dasharray is not supported yet".into());
            }
        }

        "stroke-dashoffset" => {
            if value != "inherit" {
                let offset = parse_length(value, "stroke-dashoffset")?;

                if offset != 0.0 {
                    return Err("non-zero stroke-dashoffset is not supported yet".into());
                }
            }
        }

        "paint-order" => {
            if value != "normal" && value != "inherit" {
                return Err("non-default paint-order is not supported yet".into());
            }
        }

        "vector-effect" => {
            if value != "none" && value != "inherit" {
                return Err(format!("vector-effect `{value}` is not supported yet"));
            }
        }

        "display" | "visibility" => {
            return Err(format!("SVG `{property}` is not supported yet"));
        }

        "color" => {
            return Err("SVG-local `color` is not supported yet".into());
        }

        other => {
            return Err(format!("unsupported inline SVG style property `{other}`"));
        }
    }

    Ok(())
}

fn parse_paint(value: &str, inherited: Option<ParsedPaint>) -> Result<Option<ParsedPaint>, String> {
    let paint =
        Paint::from_str(value).map_err(|error| format!("invalid SVG paint `{value}`: {error}"))?;

    match paint {
        Paint::None => Ok(None),

        Paint::Inherit => Ok(inherited),

        Paint::CurrentColor => Ok(Some(ParsedPaint::CurrentColor)),

        Paint::Color(color) => Ok(Some(ParsedPaint::Color(color.red, color.green, color.blue))),

        Paint::FuncIRI(_, _) => {
            Err("SVG paint servers such as gradients and patterns are not supported yet".into())
        }

        Paint::ContextFill | Paint::ContextStroke => {
            Err("context-fill/context-stroke are not supported yet".into())
        }
    }
}

fn is_one(value: &str) -> bool {
    value.parse::<f32>().is_ok_and(|value| value == 1.0)
}

fn parse_shape(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    match node.tag_name().name() {
        "path" => parse_path(node),
        "line" => parse_line(node),
        "polyline" => parse_polyline(node, false),
        "polygon" => parse_polyline(node, true),
        "rect" => parse_rect(node),
        "circle" => parse_circle(node),
        "ellipse" => parse_ellipse(node),

        other => Err(format!("unsupported SVG shape <{other}>")),
    }
}

fn parse_path(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    let Some(data) = node.attribute("d") else {
        return Ok(Vec::new());
    };

    if data.trim().is_empty() {
        return Ok(Vec::new());
    }

    parse_path_data(data)
}

fn parse_path_data(data: &str) -> Result<Vec<ParsedCommand>, String> {
    let mut commands = Vec::new();

    let mut current = (0.0f64, 0.0f64);
    let mut contour_start = (0.0f64, 0.0f64);

    let mut previous_cubic_control = None;
    let mut previous_quadratic_control = None;

    for segment in PathParser::from(data) {
        let segment = segment.map_err(|error| format!("invalid path data: {error}"))?;

        match segment {
            PathSegment::MoveTo { abs, x, y } => {
                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::MoveTo(
                    checked_f32(to.0, "path x")?,
                    checked_f32(to.1, "path y")?,
                ));

                current = to;
                contour_start = to;

                previous_cubic_control = None;
                previous_quadratic_control = None;
            }

            PathSegment::LineTo { abs, x, y } => {
                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::LineTo(
                    checked_f32(to.0, "path x")?,
                    checked_f32(to.1, "path y")?,
                ));

                current = to;

                previous_cubic_control = None;
                previous_quadratic_control = None;
            }

            PathSegment::HorizontalLineTo { abs, x } => {
                let x = if abs { x } else { current.0 + x };

                let to = (x, current.1);

                commands.push(ParsedCommand::LineTo(
                    checked_f32(to.0, "path x")?,
                    checked_f32(to.1, "path y")?,
                ));

                current = to;

                previous_cubic_control = None;
                previous_quadratic_control = None;
            }

            PathSegment::VerticalLineTo { abs, y } => {
                let y = if abs { y } else { current.1 + y };

                let to = (current.0, y);

                commands.push(ParsedCommand::LineTo(
                    checked_f32(to.0, "path x")?,
                    checked_f32(to.1, "path y")?,
                ));

                current = to;

                previous_cubic_control = None;
                previous_quadratic_control = None;
            }

            PathSegment::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let control_1 = absolute_point(abs, current, x1, y1);

                let control_2 = absolute_point(abs, current, x2, y2);

                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::CubicTo {
                    control_1: checked_point(control_1, "cubic control 1")?,
                    control_2: checked_point(control_2, "cubic control 2")?,
                    to: checked_point(to, "cubic endpoint")?,
                });

                current = to;

                previous_cubic_control = Some(control_2);

                previous_quadratic_control = None;
            }

            PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                let control_1 = match previous_cubic_control {
                    Some(previous) => (current.0 * 2.0 - previous.0, current.1 * 2.0 - previous.1),

                    None => current,
                };

                let control_2 = absolute_point(abs, current, x2, y2);

                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::CubicTo {
                    control_1: checked_point(control_1, "smooth cubic control 1")?,
                    control_2: checked_point(control_2, "smooth cubic control 2")?,
                    to: checked_point(to, "smooth cubic endpoint")?,
                });

                current = to;

                previous_cubic_control = Some(control_2);

                previous_quadratic_control = None;
            }

            PathSegment::Quadratic { abs, x1, y1, x, y } => {
                let control = absolute_point(abs, current, x1, y1);

                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::QuadraticTo {
                    control: checked_point(control, "quadratic control")?,
                    to: checked_point(to, "quadratic endpoint")?,
                });

                current = to;

                previous_quadratic_control = Some(control);

                previous_cubic_control = None;
            }

            PathSegment::SmoothQuadratic { abs, x, y } => {
                let control = match previous_quadratic_control {
                    Some(previous) => (current.0 * 2.0 - previous.0, current.1 * 2.0 - previous.1),

                    None => current,
                };

                let to = absolute_point(abs, current, x, y);

                commands.push(ParsedCommand::QuadraticTo {
                    control: checked_point(control, "smooth quadratic control")?,
                    to: checked_point(to, "smooth quadratic endpoint")?,
                });

                current = to;

                previous_quadratic_control = Some(control);

                previous_cubic_control = None;
            }

            PathSegment::EllipticalArc { .. } => {
                return Err("SVG path arc commands (`A`/`a`) are not supported yet".into());
            }

            PathSegment::ClosePath { .. } => {
                commands.push(ParsedCommand::Close);

                current = contour_start;

                previous_cubic_control = None;
                previous_quadratic_control = None;
            }
        }
    }

    Ok(commands)
}

fn absolute_point(absolute: bool, current: (f64, f64), x: f64, y: f64) -> (f64, f64) {
    if absolute {
        (x, y)
    } else {
        (current.0 + x, current.1 + y)
    }
}

fn parse_line(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    let x1 = geometry_length(node, "x1", 0.0)?;
    let y1 = geometry_length(node, "y1", 0.0)?;
    let x2 = geometry_length(node, "x2", 0.0)?;
    let y2 = geometry_length(node, "y2", 0.0)?;

    Ok(vec![
        ParsedCommand::MoveTo(checked_f32(x1, "line x1")?, checked_f32(y1, "line y1")?),
        ParsedCommand::LineTo(checked_f32(x2, "line x2")?, checked_f32(y2, "line y2")?),
    ])
}

fn parse_polyline(node: Node<'_, '_>, close: bool) -> Result<Vec<ParsedCommand>, String> {
    let Some(points) = node.attribute("points") else {
        return Ok(Vec::new());
    };

    let numbers = NumberListParser::from(points)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("invalid points list `{points}`: {error}"))?;

    if numbers.len() % 2 != 0 {
        return Err("polygon/polyline points must contain coordinate pairs".into());
    }

    if numbers.len() < 4 {
        return Ok(Vec::new());
    }

    let mut pairs = numbers.chunks_exact(2);

    let first = pairs.next().expect("points length checked above");

    let mut commands = vec![ParsedCommand::MoveTo(
        checked_f32(first[0], "points x")?,
        checked_f32(first[1], "points y")?,
    )];

    for point in pairs {
        commands.push(ParsedCommand::LineTo(
            checked_f32(point[0], "points x")?,
            checked_f32(point[1], "points y")?,
        ));
    }

    if close {
        commands.push(ParsedCommand::Close);
    }

    Ok(commands)
}

fn parse_rect(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    let x = geometry_length(node, "x", 0.0)?;
    let y = geometry_length(node, "y", 0.0)?;
    let width = geometry_length(node, "width", 0.0)?;
    let height = geometry_length(node, "height", 0.0)?;

    if width < 0.0 || height < 0.0 {
        return Err("<rect> width and height must not be negative".into());
    }

    if width == 0.0 || height == 0.0 {
        return Ok(Vec::new());
    }

    let rx = node
        .attribute("rx")
        .map(|value| parse_length(value, "rect rx"))
        .transpose()?;

    let ry = node
        .attribute("ry")
        .map(|value| parse_length(value, "rect ry"))
        .transpose()?;

    let (mut rx, mut ry) = match (rx, ry) {
        (Some(rx), Some(ry)) => (rx, ry),
        (Some(rx), None) => (rx, rx),
        (None, Some(ry)) => (ry, ry),
        (None, None) => (0.0, 0.0),
    };

    if rx < 0.0 || ry < 0.0 {
        return Err("<rect> rx and ry must not be negative".into());
    }

    rx = rx.min(width / 2.0);
    ry = ry.min(height / 2.0);

    if rx == 0.0 || ry == 0.0 {
        return rectangle_commands(x, y, width, height);
    }

    rounded_rectangle_commands(x, y, width, height, rx, ry)
}

fn rectangle_commands(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<Vec<ParsedCommand>, String> {
    Ok(vec![
        move_to(x, y)?,
        line_to(x + width, y)?,
        line_to(x + width, y + height)?,
        line_to(x, y + height)?,
        ParsedCommand::Close,
    ])
}

fn rounded_rectangle_commands(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rx: f64,
    ry: f64,
) -> Result<Vec<ParsedCommand>, String> {
    let right = x + width;
    let bottom = y + height;

    let kx = rx * ELLIPSE_KAPPA;
    let ky = ry * ELLIPSE_KAPPA;

    Ok(vec![
        move_to(x + rx, y)?,
        line_to(right - rx, y)?,
        cubic_to((right - rx + kx, y), (right, y + ry - ky), (right, y + ry))?,
        line_to(right, bottom - ry)?,
        cubic_to(
            (right, bottom - ry + ky),
            (right - rx + kx, bottom),
            (right - rx, bottom),
        )?,
        line_to(x + rx, bottom)?,
        cubic_to(
            (x + rx - kx, bottom),
            (x, bottom - ry + ky),
            (x, bottom - ry),
        )?,
        line_to(x, y + ry)?,
        cubic_to((x, y + ry - ky), (x + rx - kx, y), (x + rx, y))?,
        ParsedCommand::Close,
    ])
}

fn parse_circle(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    let cx = geometry_length(node, "cx", 0.0)?;
    let cy = geometry_length(node, "cy", 0.0)?;
    let radius = geometry_length(node, "r", 0.0)?;

    if radius < 0.0 {
        return Err("<circle> radius must not be negative".into());
    }

    if radius == 0.0 {
        return Ok(Vec::new());
    }

    ellipse_commands(cx, cy, radius, radius)
}

fn parse_ellipse(node: Node<'_, '_>) -> Result<Vec<ParsedCommand>, String> {
    let cx = geometry_length(node, "cx", 0.0)?;
    let cy = geometry_length(node, "cy", 0.0)?;
    let rx = geometry_length(node, "rx", 0.0)?;
    let ry = geometry_length(node, "ry", 0.0)?;

    if rx < 0.0 || ry < 0.0 {
        return Err("<ellipse> radii must not be negative".into());
    }

    if rx == 0.0 || ry == 0.0 {
        return Ok(Vec::new());
    }

    ellipse_commands(cx, cy, rx, ry)
}

fn ellipse_commands(cx: f64, cy: f64, rx: f64, ry: f64) -> Result<Vec<ParsedCommand>, String> {
    let kx = rx * ELLIPSE_KAPPA;
    let ky = ry * ELLIPSE_KAPPA;

    Ok(vec![
        move_to(cx + rx, cy)?,
        cubic_to((cx + rx, cy + ky), (cx + kx, cy + ry), (cx, cy + ry))?,
        cubic_to((cx - kx, cy + ry), (cx - rx, cy + ky), (cx - rx, cy))?,
        cubic_to((cx - rx, cy - ky), (cx - kx, cy - ry), (cx, cy - ry))?,
        cubic_to((cx + kx, cy - ry), (cx + rx, cy - ky), (cx + rx, cy))?,
        ParsedCommand::Close,
    ])
}

fn move_to(x: f64, y: f64) -> Result<ParsedCommand, String> {
    Ok(ParsedCommand::MoveTo(
        checked_f32(x, "x")?,
        checked_f32(y, "y")?,
    ))
}

fn line_to(x: f64, y: f64) -> Result<ParsedCommand, String> {
    Ok(ParsedCommand::LineTo(
        checked_f32(x, "x")?,
        checked_f32(y, "y")?,
    ))
}

fn cubic_to(
    control_1: (f64, f64),
    control_2: (f64, f64),
    to: (f64, f64),
) -> Result<ParsedCommand, String> {
    Ok(ParsedCommand::CubicTo {
        control_1: checked_point(control_1, "cubic control 1")?,
        control_2: checked_point(control_2, "cubic control 2")?,
        to: checked_point(to, "cubic endpoint")?,
    })
}

fn geometry_length(node: Node<'_, '_>, attribute: &str, default: f64) -> Result<f64, String> {
    match node.attribute(attribute) {
        Some(value) => parse_length(value, attribute),

        None => Ok(default),
    }
}

fn parse_length(value: &str, description: &str) -> Result<f64, String> {
    let length = Length::from_str(value)
        .map_err(|error| format!("invalid {description} `{value}`: {error}"))?;

    match length.unit {
        LengthUnit::None | LengthUnit::Px => {}

        other => {
            return Err(format!(
                "{description} uses unsupported SVG unit {other:?}; \
                 only unitless values and px are supported"
            ));
        }
    }

    if !length.number.is_finite() {
        return Err(format!("{description} must be finite"));
    }

    Ok(length.number)
}

fn checked_point(point: (f64, f64), description: &str) -> Result<(f32, f32), String> {
    Ok((
        checked_f32(point.0, &format!("{description} x"))?,
        checked_f32(point.1, &format!("{description} y"))?,
    ))
}

fn checked_f32(value: f64, description: &str) -> Result<f32, String> {
    if !value.is_finite() || value < -(f32::MAX as f64) || value > f32::MAX as f64 {
        return Err(format!("{description} is outside the supported f32 range"));
    }

    Ok(value as f32)
}

fn intrinsic_pixel_size(value: f64, description: &str) -> Result<i32, String> {
    if !value.is_finite() || value <= 0.0 || value > i32::MAX as f64 {
        return Err(format!(
            "{description} is outside the supported pixel range"
        ));
    }

    Ok((value.round() as i32).max(1))
}

fn emit_svg(svg: ParsedSvg, dependency_path: LitStr) -> TokenStream {
    let ParsedSvg {
        width,
        height,
        view_box,
        paths,
    } = svg;

    let [view_min_x, view_min_y, view_width, view_height] = view_box;

    let mut command_statics = Vec::new();
    let mut path_expressions = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        let command_ident = format_ident!("__INKPAPER_SVG_COMMANDS_{index}");

        let command_count = path.commands.len();

        let commands = path.commands.iter().map(command_tokens).collect::<Vec<_>>();

        command_statics.push(quote! {
            static #command_ident: [::inkpaper_ui::PathCommand; #command_count] = [ #(#commands),* ];
        });

        let mut expression = quote! {
            ::inkpaper_ui::SvgPath::new(
                ::inkpaper_ui::VectorPath::new(&#command_ident)
            )
        };

        if let Some(fill) = path.fill {
            let fill = fill_tokens(fill);

            expression = quote! { #expression.with_fill(#fill) };
        }

        if let Some(stroke) = path.stroke {
            let stroke = stroke_tokens(stroke);

            expression = quote! { #expression.with_stroke(#stroke) };
        }

        path_expressions.push(expression);
    }

    let path_count = path_expressions.len();

    quote! {{
        // make this input visible to rustc's dependency tracking.
        // if the SVG changes, this crate must be rebuilt and the
        // procedural macro will be expanded again.
        const _: &str = include_str!(#dependency_path);

        #(#command_statics)*

        static __INKPAPER_SVG_PATHS: [::inkpaper_ui::SvgPath; #path_count] = [ #(#path_expressions),* ];

        ::inkpaper_ui::SvgSource::new(
            ::inkpaper_ui::Size::new(
                ::inkpaper_ui::px(#width),
                ::inkpaper_ui::px(#height),
            ),
            ::inkpaper_ui::SvgViewBox::new(
                #view_min_x,
                #view_min_y,
                #view_width,
                #view_height,
            ),
            &__INKPAPER_SVG_PATHS,
        )
    }}
}

fn command_tokens(command: &ParsedCommand) -> TokenStream {
    match *command {
        ParsedCommand::MoveTo(x, y) => quote! {
            ::inkpaper_ui::PathCommand::MoveTo(
                ::inkpaper_ui::VectorPoint::new(#x, #y)
            )
        },
        ParsedCommand::LineTo(x, y) => quote! {
            ::inkpaper_ui::PathCommand::LineTo(
                ::inkpaper_ui::VectorPoint::new(#x, #y)
            )
        },
        ParsedCommand::QuadraticTo {
            control: (cx, cy),
            to: (x, y),
        } => quote! {
            ::inkpaper_ui::PathCommand::QuadraticTo {
                control: ::inkpaper_ui::VectorPoint::new(#cx, #cy),
                to: ::inkpaper_ui::VectorPoint::new(#x, #y),
            }
        },
        ParsedCommand::CubicTo {
            control_1: (c1x, c1y),
            control_2: (c2x, c2y),
            to: (x, y),
        } => quote! {
            ::inkpaper_ui::PathCommand::CubicTo {
                control_1: ::inkpaper_ui::VectorPoint::new(#c1x, #c1y),
                control_2: ::inkpaper_ui::VectorPoint::new(#c2x, #c2y),
                to: ::inkpaper_ui::VectorPoint::new(#x, #y),
            }
        },
        ParsedCommand::Close => quote! { ::inkpaper_ui::PathCommand::Close },
    }
}

fn paint_tokens(paint: ParsedPaint) -> TokenStream {
    match paint {
        ParsedPaint::CurrentColor => quote! { ::inkpaper_ui::SvgPaint::CurrentColor },
        ParsedPaint::Color(red, green, blue) => quote! {
            ::inkpaper_ui::SvgPaint::Color(
                ::inkpaper_ui::Color::rgb(#red, #green, #blue)
            )
        },
    }
}

fn fill_tokens(fill: ParsedFill) -> TokenStream {
    let paint = paint_tokens(fill.paint);

    let rule = match fill.rule {
        ParsedFillRule::NonZero => quote! { ::inkpaper_ui::FillRule::NonZero },
        ParsedFillRule::EvenOdd => quote! { ::inkpaper_ui::FillRule::EvenOdd },
    };

    quote! { ::inkpaper_ui::SvgFill::new(#paint) .with_rule(#rule) }
}

fn stroke_tokens(stroke: ParsedStroke) -> TokenStream {
    let paint = paint_tokens(stroke.paint);

    let width = stroke.width;
    let miter_limit = stroke.miter_limit;

    let cap = match stroke.cap {
        ParsedStrokeCap::Butt => quote! { ::inkpaper_ui::StrokeCap::Butt },
        ParsedStrokeCap::Round => quote! { ::inkpaper_ui::StrokeCap::Round },
        ParsedStrokeCap::Square => quote! { ::inkpaper_ui::StrokeCap::Square },
    };

    let join = match stroke.join {
        ParsedStrokeJoin::Miter => quote! { ::inkpaper_ui::StrokeJoin::Miter },
        ParsedStrokeJoin::Round => quote! { ::inkpaper_ui::StrokeJoin::Round },
        ParsedStrokeJoin::Bevel => quote! { ::inkpaper_ui::StrokeJoin::Bevel },
    };

    quote! {
        ::inkpaper_ui::SvgStroke::new(#width, #paint)
            .with_cap(#cap)
            .with_join(#join)
            .with_miter_limit(#miter_limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lucide_style_svg() {
        let svg = parse_svg(
            r#"
                <svg
                    width="24"
                    height="24"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M3 12h18" />
                    <circle cx="12" cy="12" r="3" />
                </svg>
            "#,
        )
        .unwrap();

        assert_eq!(svg.width, 24);
        assert_eq!(svg.height, 24);
        assert_eq!(svg.view_box, [0.0, 0.0, 24.0, 24.0],);
        assert_eq!(svg.paths.len(), 2);
        assert_eq!(
            svg.paths[0].commands,
            vec![
                ParsedCommand::MoveTo(3.0, 12.0,),
                ParsedCommand::LineTo(21.0, 12.0,),
            ],
        );
        assert!(svg.paths[0].fill.is_none());
        assert_eq!(
            svg.paths[0].stroke,
            Some(ParsedStroke {
                paint: ParsedPaint::CurrentColor,
                width: 2.0,
                cap: ParsedStrokeCap::Round,
                join: ParsedStrokeJoin::Round,
                miter_limit: 4.0,
            }),
        );
        assert_eq!(svg.paths[1].commands.len(), 6,);
    }

    #[test]
    fn svg_defaults_to_black_fill() {
        let svg = parse_svg(
            r#"
                <svg viewBox="0 0 10 10">
                    <rect
                        x="1"
                        y="2"
                        width="3"
                        height="4"
                    />
                </svg>
            "#,
        )
        .unwrap();

        assert_eq!(svg.paths.len(), 1);
        assert_eq!(
            svg.paths[0].fill,
            Some(ParsedFill {
                paint: ParsedPaint::Color(0, 0, 0,),
                rule: ParsedFillRule::NonZero,
            }),
        );
        assert!(svg.paths[0].stroke.is_none());
    }

    #[test]
    fn group_styles_are_inherited_and_inline_style_wins() {
        let svg = parse_svg(
            r##"
                <svg
                    viewBox="0 0 20 20"
                    fill="none"
                    stroke="#112233"
                    stroke-width="2"
                >
                    <g stroke-linecap="round">
                        <path d="M0 0L10 10" style="stroke-width: 3" />
                    </g>
                </svg>
            "##,
        )
        .unwrap();

        assert_eq!(svg.paths.len(), 1);
        assert_eq!(
            svg.paths[0].stroke,
            Some(ParsedStroke {
                paint: ParsedPaint::Color(0x11, 0x22, 0x33,),
                width: 3.0,
                cap: ParsedStrokeCap::Round,
                join: ParsedStrokeJoin::Miter,
                miter_limit: 4.0,
            }),
        );
    }

    #[test]
    fn relative_and_smooth_commands_are_normalized() {
        let commands = parse_path_data("M10 10c1 2 3 4 5 6s7 8 9 10q1 2 3 4t5 6z").unwrap();

        assert!(matches!(commands[0], ParsedCommand::MoveTo(10.0, 10.0,)));
        assert!(matches!(commands[1], ParsedCommand::CubicTo { .. }));
        assert!(matches!(commands[2], ParsedCommand::CubicTo { .. }));
        assert!(matches!(commands[3], ParsedCommand::QuadraticTo { .. }));
        assert!(matches!(commands[4], ParsedCommand::QuadraticTo { .. }));
        assert_eq!(commands.last(), Some(&ParsedCommand::Close),);
    }

    #[test]
    fn rejects_arc_commands_explicitly() {
        let error = parse_path_data("M0 0 A10 10 0 0 0 20 20").unwrap_err();

        assert!(error.contains("arc commands"));
    }

    #[test]
    fn rejects_transforms_instead_of_ignoring_them() {
        let error = parse_svg(
            r#"
                <svg viewBox="0 0 24 24">
                    <g transform="translate(2 3)">
                        <path d="M0 0L1 1" />
                    </g>
                </svg>
            "#,
        )
        .unwrap_err();

        assert!(error.contains("transforms are not supported"));
    }

    #[test]
    fn rounded_rect_becomes_static_curve_path() {
        let svg = parse_svg(
            r#"
                <svg viewBox="0 0 20 20">
                    <rect
                        x="2"
                        y="3"
                        width="12"
                        height="10"
                        rx="2"
                    />
                </svg>
            "#,
        )
        .unwrap();

        let commands = &svg.paths[0].commands;

        assert!(matches!(commands[0], ParsedCommand::MoveTo(..)));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, ParsedCommand::CubicTo { .. }))
        );
        assert_eq!(commands.last(), Some(&ParsedCommand::Close),);
    }
}
