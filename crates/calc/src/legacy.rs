//! Legacy formulas: formulas read from a file without dynamic-array metadata (see
//! `Formula::legacy`). Excel calculates them with implicit intersection and never spills them.
//!
//! The formula's tree is rewritten for calculation only (the stored text and tree are unchanged):
//! an implicit intersection `@` goes wherever a single value is expected, so the evaluator's `@`
//! reduces a multi-cell reference to the cell in the formula's row or column (`#VALUE!` when there
//! is none), and an array to its first value. Single values are expected
//!
//! - by the operands of the unary and binary operators (other than the reference operators `:`,
//!   `,` and space, which build references);
//! - by the parameters of functions that take a value ([`FnSpec::is_scalar_arg`], and
//!   [`value_param`] for the functions the evaluator implements itself);
//! - as the formula's result.
//!
//! An argument of a parameter that takes a reference or an array (SUM's numbers, SUMPRODUCT,
//! INDEX's array, COUNTIF's range) is left as it is, so ranges and the expressions in it are
//! evaluated as arrays.
//!
//! [`FnSpec::is_scalar_arg`]: gridcraft_functions::FnSpec::is_scalar_arg

use gridcraft_formula::{BinOp, Expr, RefKind, SheetSel, UnOp};

/// Nesting past which subtrees are left as they are. The parser nests at most 256 levels (an
/// operator chain such as `A1+A2+…` is followed in a loop, not by recursion), so this only
/// bounds the stack for trees built some other way.
const MAX_DEPTH: usize = 320;

/// `expr` with implicit intersection made explicit where Excel applies it to a legacy formula.
pub(crate) fn legacy_expr(expr: &Expr) -> Expr {
    value(expr.clone(), 0)
}

/// `e` rewritten for a place that expects a single value.
fn value(e: Expr, depth: usize) -> Expr {
    if depth > MAX_DEPTH {
        return e;
    }
    match e {
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing => e,
        Expr::Ref(ref r) if matches!(r.kind, RefKind::Cell(_)) && !matches!(r.sheet, SheetSel::Span(_, _)) => e,
        Expr::Unary(UnOp::At, _) => e,
        Expr::Unary(op @ (UnOp::Neg | UnOp::Plus | UnOp::Percent), x) => Expr::Unary(op, Box::new(value(*x, depth + 1))),
        Expr::Paren(x) => Expr::Paren(Box::new(value(*x, depth + 1))),
        Expr::Binary(op, a, b) if !is_reference_op(op) => {
            // A flat chain such as A1+A2+… nests one level per operator on its left: unwind it
            // in a loop rather than recursing once per operand.
            let mut spine = vec![(op, *b)];
            let mut left = *a;
            loop {
                match left {
                    Expr::Binary(op, a, b) if !is_reference_op(op) => {
                        spine.push((op, *b));
                        left = *a;
                    }
                    other => {
                        left = other;
                        break;
                    }
                }
            }
            let mut acc = value(left, depth + 1);
            while let Some((op, b)) = spine.pop() {
                acc = Expr::Binary(op, Box::new(acc), Box::new(value(b, depth + 1)));
            }
            acc
        }
        Expr::Call(name, args) => {
            let args = args.into_iter().enumerate().map(|(i, a)| if value_param(&name, i) { value(a, depth + 1) } else { a }).collect();
            at(Expr::Call(name, args))
        }
        // References, names, structured references, array constants, the reference operators,
        // spill and trim references, and lambda calls.
        other => at(other),
    }
}

fn at(e: Expr) -> Expr {
    Expr::Unary(UnOp::At, Box::new(e))
}

fn is_reference_op(op: BinOp) -> bool {
    matches!(op, BinOp::Range | BinOp::Union | BinOp::Intersect)
}

/// Whether argument `i` of `name` is taken as a single value (in a place that expects one).
/// The functions the evaluator implements itself are listed here; the others follow the function
/// registry's scalar parameters. Unknown functions and the LAMBDA family keep their arguments.
fn value_param(name: &str, i: usize) -> bool {
    match name {
        // The condition, and the results, which become the call's own (single) result.
        "IF" | "IFS" | "IFERROR" | "IFNA" | "CHOOSE" | "SWITCH" | "INDIRECT" | "TEXT" => true,
        // The reference parameter keeps the reference; the positions and sizes are values.
        "INDEX" | "OFFSET" => i != 0,
        "CELL" | "SUBTOTAL" => i == 0,
        "AGGREGATE" => i < 2,
        "ROW" | "COLUMN" | "ROWS" | "COLUMNS" | "AREAS" | "ISREF" | "ISFORMULA" | "FORMULATEXT" | "SHEET" | "SHEETS" | "TRIMRANGE" | "LET"
        | "LAMBDA" | "ISOMITTED" | "MAP" | "REDUCE" | "SCAN" | "BYROW" | "BYCOL" | "MAKEARRAY" => false,
        _ => gridcraft_functions::lookup(name).is_some_and(|spec| spec.is_scalar_arg(i)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewritten(text: &str) -> String {
        gridcraft_formula::parse(text).map(|e| gridcraft_formula::print(&legacy_expr(&e))).unwrap_or_default()
    }

    #[test]
    fn single_values_are_expected_by_operators_value_parameters_and_the_result() {
        assert_eq!(rewritten("A1:A5"), "@A1:A5");
        assert_eq!(rewritten("A1+B1"), "A1+B1");
        assert_eq!(rewritten("A1:A5*2"), "@A1:A5*2");
        assert_eq!(rewritten("C7+G7:J7"), "C7+@G7:J7");
        assert_eq!(rewritten("SUM(A1:A5)"), "@SUM(A1:A5)");
        assert_eq!(rewritten("SUMPRODUCT(A1:A5*B1:B5)"), "@SUMPRODUCT(A1:A5*B1:B5)");
        assert_eq!(rewritten("COUNTIF(A1:A5,A1:A5>25)"), "@COUNTIF(A1:A5,@A1:A5>25)");
        assert_eq!(rewritten("INDEX(A1:B5,0,2)"), "@INDEX(A1:B5,0,2)");
        // The intersection operator builds a reference, which is then intersected as a whole.
        let e = gridcraft_formula::parse("A:A B:B").map(|e| legacy_expr(&e));
        assert!(matches!(e, Ok(Expr::Unary(UnOp::At, x)) if matches!(*x, Expr::Binary(BinOp::Intersect, _, _))));
    }

    #[test]
    fn a_long_operator_chain_is_rewritten_without_deep_recursion() {
        let text = (1..=4000).map(|r| format!("A{r}:B{r}")).collect::<Vec<_>>().join("+");
        let Ok(e) = gridcraft_formula::parse(&text) else { return };
        let mut ats = 0;
        legacy_expr(&e).walk(&mut |x| ats += usize::from(matches!(x, Expr::Unary(UnOp::At, _))));
        assert_eq!(ats, 4000);
    }
}
