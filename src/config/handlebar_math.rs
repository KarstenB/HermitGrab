use handlebars::{
    Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError,
    RenderErrorReason,
};

pub fn math_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let p0 = h.param(0);
    let p1 = h.param(1);
    let p2 = h.param(2);

    if p0.is_none() {
        return Err(RenderErrorReason::Other(
            "At least one parameter is required for math helper".to_string(),
        )
        .into());
    }

    // Check if it's an infix operation (val op val)
    let is_infix = p1
        .and_then(|v| v.value().as_str())
        .map_or(false, |s| is_unary_operator(s));

    if is_infix {
        let Some(operator) = p1 else {
            return Err(RenderErrorReason::Other(
                "Second parameter must be an operator string".to_string(),
            )
            .into());
        };
        let Some(operator) = operator.value().as_str() else {
            return Err(RenderErrorReason::Other(
                "Operator parameter must be a string".to_string(),
            )
            .into());
        };
        let l_val = get_f64(p0, &format!("left hand side of operator '{operator}'"))?;

        // --- Special Handling for "format" ---
        if operator == "format" {
            // Get the format string (e.g., "{:05.2}") or default to "{}"
            let fmt_str = p2.and_then(|v| v.value().as_str()).unwrap_or("{}");

            // Parse logic: Extract padding, width, and precision
            let (zero_pad, width, precision) = parse_rust_format(fmt_str);

            // Apply the correct dynamic formatting
            match (zero_pad, width, precision) {
                // Case: {:0W.P} -> Zero pad, Width, Precision
                (true, Some(w), Some(p)) => {
                    write!(out, "{:0width$.prec$}", l_val, width = w, prec = p)?
                }
                // Case: {:0W} -> Zero pad, Width
                (true, Some(w), None) => write!(out, "{:0width$}", l_val, width = w)?,
                // Case: {:W.P} -> Space pad, Width, Precision
                (false, Some(w), Some(p)) => {
                    write!(out, "{:width$.prec$}", l_val, width = w, prec = p)?
                }
                // Case: {:W} -> Space pad, Width
                (false, Some(w), None) => write!(out, "{:width$}", l_val, width = w)?,
                // Case: {:.P} -> Precision only
                (false, None, Some(p)) => write!(out, "{:.prec$}", l_val, prec = p)?,
                // Default
                _ => write!(out, "{}", l_val)?,
            }
        } else {
            // --- Standard Math Operations ---
            let r_val = get_f64(p2, &format!("right hand sight of operator '{operator}'"))?; // For math, 2nd arg is a number
            match operator {
                "+" => write!(out, "{}", l_val + r_val)?,
                "-" => write!(out, "{}", l_val - r_val)?,
                "*" => write!(out, "{}", l_val * r_val)?,
                "/" => write!(out, "{}", if r_val == 0.0 { 0.0 } else { l_val / r_val })?,
                "%" => write!(out, "{}", l_val % r_val)?,
                "max" => write!(out, "{}", l_val.max(r_val))?,
                "min" => write!(out, "{}", l_val.min(r_val))?,
                "&" => write!(out, "{}", (l_val as i64 & r_val as i64))?,
                "|" => write!(out, "{}", (l_val as i64 | r_val as i64))?,
                "^" => write!(out, "{}", (l_val as i64 ^ r_val as i64))?,
                "<<" => write!(out, "{}", (l_val as i64) << (r_val as i64))?,
                ">>" => write!(out, "{}", (l_val as i64) >> (r_val as i64))?,
                _ => {
                    return Err(RenderErrorReason::Other(format!(
                        "Unsupported operator: {operator}"
                    ))
                    .into());
                }
            }
        }
    } else {
        // --- Unary Operations ---
        let operator = p0.and_then(|v| v.value().as_str()).ok_or_else(|| {
            RenderErrorReason::Other(format!(
                "First argument to unary math function is not a string: {p0:#?}"
            ))
        })?;
        let val = get_f64(p1, "'{operator}' argument")?;

        match operator {
            "abs" => write!(out, "{}", val.abs())?,
            "ceil" => write!(out, "{}", val.ceil())?,
            "floor" => write!(out, "{}", val.floor())?,
            "round" => write!(out, "{}", val.round())?,
            "sqrt" => write!(out, "{}", val.sqrt())?,
            "not" | "~" => write!(out, "{}", !(val as i64))?,
            _ => {
                return Err(RenderErrorReason::Other(format!(
                    "Unsupported unary operator: {operator}"
                ))
                .into());
            }
        }
    };

    Ok(())
}

// --- Helper: Parse rust-style format string ---
// Input examples: "{:.2}", "{:05.2}", "05", ".2"
fn parse_rust_format(s: &str) -> (bool, Option<usize>, Option<usize>) {
    // 1. Clean wrappers "{: ... }"
    let clean = s.trim_matches(|c| c == '{' || c == '}' || c == ':');

    // 2. Check for zero padding (if string starts with '0' and has other digits)
    // Note: "0.2" implies zero padding only if width is present before dot.
    // Simplification: If it starts with 0 and isn't just "0", treat as zero-pad.
    let (zero_pad, rest) = if clean.starts_with('0') && clean.len() > 1 && !clean.starts_with("0.")
    {
        (true, &clean[1..])
    } else {
        (false, clean)
    };

    // 3. Split by '.' to get Width and Precision
    if let Some((w_str, p_str)) = rest.split_once('.') {
        // Has a dot: Left is Width (opt), Right is Precision
        let width = w_str.parse::<usize>().ok();
        let precision = p_str.parse::<usize>().ok();
        (zero_pad, width, precision)
    } else {
        // No dot: It is just Width
        let width = rest.parse::<usize>().ok();
        (zero_pad, width, None)
    }
}

// --- Utilities ---
fn is_unary_operator(s: &str) -> bool {
    matches!(
        s,
        "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>" | "max" | "min" | "format"
    )
}

fn get_f64(param: Option<&handlebars::PathAndJson>, context: &str) -> Result<f64, RenderError> {
    if let Some(v) = param {
        if let Some(n) = v.value().as_f64() {
            return Ok(n);
        }
        if let Some(s) = v.value().as_str() {
            if let Ok(n) = s.parse::<f64>() {
                return Ok(n);
            } else {
                return Err(RenderErrorReason::Other(format!(
                    "Failed to parse '{}' as a number for {context}",
                    s
                ))
                .into());
            }
        }
        return Err(RenderErrorReason::Other(format!(
            "Expected a number or numeric string for {context}, got: {}",
            v.value()
        ))
        .into());
    }
    Err(RenderErrorReason::Other(format!("Parameter for {context} is absent")).into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn render(template: &str, data: &serde_json::Value) -> String {
        let mut hb = Handlebars::new();
        hb.register_helper("math", Box::new(math_helper));
        hb.render_template(template, data).unwrap()
    }

    #[test]
    fn test_basic_arithmetic() {
        let data = json!({"a": 10, "b": 2});
        assert_eq!(render("{{math a '+' b}}", &data), "12");
        assert_eq!(render("{{math a '-' b}}", &data), "8");
        assert_eq!(render("{{math a '*' b}}", &data), "20");
        assert_eq!(render("{{math a '/' b}}", &data), "5");
        assert_eq!(render("{{math 10 '%' 3}}", &data), "1");
    }

    #[test]
    fn test_formatting() {
        let data = json!({"val": 3.14159});
        // "format" op takes value as left, precision as right
        assert_eq!(render("{{math val 'format' '.2'}}", &data), "3.14");
        assert_eq!(render("{{math val 'format' '.0'}}", &data), "3");
        assert_eq!(render("{{math 100 'format' '.2'}}", &data), "100.00");
    }

    #[test]
    fn test_unary_math() {
        let data = json!({"neg": -5.5, "float": 5.1});
        assert_eq!(render("{{math 'abs' neg}}", &data), "5.5");
        assert_eq!(render("{{math 'ceil' float}}", &data), "6");
        assert_eq!(render("{{math 'floor' float}}", &data), "5");
        assert_eq!(render("{{math 'round' 5.6}}", &data), "6");
        assert_eq!(render("{{math 'sqrt' 9}}", &data), "3");
    }

    #[test]
    fn test_bitwise_ops() {
        // bitwise ops cast to i64 internally
        let data = json!({"x": 12, "y": 5}); // 1100, 0101
        assert_eq!(render("{{math x '&' y}}", &data), "4"); // 0100
        assert_eq!(render("{{math x '|' y}}", &data), "13"); // 1101
        assert_eq!(render("{{math x '^' y}}", &data), "9"); // 1001
        assert_eq!(render("{{math 1 '<<' 4}}", &data), "16");
        assert_eq!(render("{{math 16 '>>' 1}}", &data), "8");
        assert_eq!(render("{{math 'not' 0}}", &data), "-1");
    }

    #[test]
    fn test_min_max() {
        assert_eq!(render("{{math 10 'max' 20}}", &json!({})), "20");
        assert_eq!(render("{{math 10 'min' 5}}", &json!({})), "5");
    }

    #[test]
    fn test_compound_nested() {
        // (a / b) + 3
        // (10 / 2) + 3 = 8
        let data = json!({"a": 10, "b": 2});
        let result = render("{{math (math a '/' b) '+' 3}}", &data);
        assert_eq!(result, "8");
    }

    #[test]
    fn test_complex_compound_with_formatting() {
        // Calculate price with tax and format it
        // Formula: format(price * 1.20, 2)
        // price = 50 -> 60.00
        let data = json!({"price": 50});
        let t = "{{math (math price '*' 1.2) 'format' '.2'}}";
        assert_eq!(render(t, &data), "60.00");
    }

    #[test]
    fn test_divide_by_zero_safety() {
        let data = json!({});
        // Should return 0 rather than panic
        assert_eq!(render("{{math 10 '/' 0}}", &data), "0");
    }

    #[test]
    fn test_format_precision() {
        let data = json!({"val": 3.14159});
        // Standard rust syntax
        assert_eq!(render("{{math val 'format' '{:.2}'}}", &data), "3.14");
        // Shorthand syntax
        assert_eq!(render("{{math val 'format' '.3'}}", &data), "3.142");
    }

    #[test]
    fn test_format_padding() {
        let data = json!({"n": 5});
        // Zero padding width 4
        assert_eq!(render("{{math n 'format' '{:04}'}}", &data), "0005");
        // Space padding width 4 (difficult to see in assert, but checks string len)
        assert_eq!(render("{{math n 'format' '{:4}'}}", &data), "   5");
    }

    #[test]
    fn test_format_combined() {
        let data = json!({"val": 3.1});
        // Width 6, Precision 2, Zero Pad
        // Expect: "003.10"
        assert_eq!(render("{{math val 'format' '{:06.2}'}}", &data), "003.10");
    }

    #[test]
    fn test_complex_expression_formatting() {
        // (10 / 3) formatted to 2 decimals
        let data = json!({});
        let result = render("{{math (math 10 '/' 3) 'format' '{:.2}'}}", &data);
        assert_eq!(result, "3.33");
    }
}
