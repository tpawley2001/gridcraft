use super::array::{Acc, Agg, broadcast_dim, broadcast_len, zip2, zip3};
use super::format::same_kind;
use super::reference_fns::RefArg;
use super::special::{ev_shape, linked_parts, resized_sum_range, resizes_sum_range, takes_reference};
use super::{Area, Arg, Array, CellError, Ev, Evaluator, Expr, FnCtx, Lambda, Pipe, Value, compare, err};
use std::sync::Arc;

impl<'h> Evaluator<'h> {
    pub(super) fn call(&mut self, name: &str, args: &[Expr]) -> Ev {
        match name {
            "IF" => self.if_call(args),
            "IFS" => {
                if args.len() < 2 || !args.len().is_multiple_of(2) {
                    return err(CellError::Value);
                }
                for pair in args.chunks(2) {
                    let [c, v] = pair else { break };
                    let cv = self.value(c);
                    match cv.scalar().to_bool() {
                        Ok(true) => return self.eval(v),
                        Ok(false) => {}
                        Err(e) => return err(e),
                    }
                }
                err(CellError::NA)
            }
            "IFERROR" | "IFNA" => self.if_error_call(name, args),
            "CHOOSE" => self.choose_call(args),
            "SWITCH" => self.switch_call(args),
            "ROW" | "COLUMN" => self.row_column_call(name, args),
            "T" => self.t_call(args),
            "ROWS" | "COLUMNS" => {
                if args.len() != 1 {
                    return err(CellError::Value);
                }
                let is_rows = name == "ROWS";
                match self.eval(&args[0]) {
                    Ev::R(a) => match a.as_slice() {
                        [a] => Ev::V(Value::number(if is_rows { a.range.height() } else { a.range.width() } as f64)),
                        _ => err(CellError::Ref),
                    },
                    Ev::V(Value::Array(arr)) => Ev::V(Value::number(if is_rows { arr.rows } else { arr.cols } as f64)),
                    Ev::V(Value::Error(e)) => err(e),
                    Ev::V(_) => Ev::V(Value::Number(1.0)),
                    Ev::L(_) => err(CellError::Value),
                }
            }
            "AREAS" => match args.first().map(|a| self.eval(a)) {
                Some(Ev::R(a)) => Ev::V(Value::number(a.len() as f64)),
                Some(Ev::V(Value::Error(e))) => err(e),
                _ => err(CellError::Value),
            },
            "ISREF" => Ev::V(Value::Bool(matches!(args.first().map(|a| self.eval(a)), Some(Ev::R(_))))),
            "ISFORMULA" | "FORMULATEXT" => {
                let Some(Ev::R(a)) = args.first().map(|a| self.eval(a)) else { return err(CellError::NA) };
                let Some(a) = a.first() else { return err(CellError::NA) };
                let wb = self.host.workbook();
                let f = wb.sheet(a.sheet).and_then(|s| s.cell(a.range.start)).and_then(|c| c.formula.clone());
                // A formula formatted Hidden on a protected sheet stays hidden from FORMULATEXT too.
                let hidden = wb.sheet(a.sheet).is_some_and(|s| s.is_protected() && wb.styles.get(s.style_id(a.range.start)).protection.hidden);
                if name == "ISFORMULA" {
                    Ev::V(Value::Bool(f.is_some()))
                } else {
                    match f {
                        Some(f) if !hidden => Ev::V(Value::text(format!("={}", f.text))),
                        _ => err(CellError::NA),
                    }
                }
            }
            "SHEET" | "SHEETS" => self.sheet_call(name, args),
            "OFFSET" => self.offset(args),
            "INDIRECT" => self.indirect(args),
            "INDEX" => self.index(args),
            "XLOOKUP" => self.xlookup(args),
            "CELL" => self.cell_info(args),
            "TRIMRANGE" => self.trimrange(args),
            "SUBTOTAL" | "AGGREGATE" => self.subtotal(name, args),
            "TEXT" => {
                if args.len() != 2 {
                    return err(CellError::Value);
                }
                let v = self.arg_val(args, 0).unwrap_or_default();
                let f = self.arg_val(args, 1).unwrap_or_default();
                let sys = self.host.workbook().date_system;
                let one = |v: &Value, f: &Value| -> Value {
                    if let Value::Error(e) = v {
                        return Value::Error(*e);
                    }
                    let code = match f.to_text() {
                        Ok(c) => c,
                        Err(e) => return Value::Error(e),
                    };
                    match gridcraft_numfmt::text_function(v, &code, sys) {
                        Ok(s) => Value::text(s),
                        Err(e) => Value::Error(e),
                    }
                };
                Ev::V(zip2(&v, &f, one))
            }
            "LET" => self.let_(args),
            "LAMBDA" => {
                if args.is_empty() {
                    return err(CellError::Value);
                }
                let mut params = Vec::new();
                for p in &args[..args.len() - 1] {
                    match p {
                        Expr::Name(n) => params.push(n.trim_start_matches('[').trim_end_matches(']').to_string()),
                        _ => return err(CellError::Value),
                    }
                }
                let Some(body) = args.last() else { return err(CellError::Value) };
                Ev::L(Arc::new(Lambda { params, body: body.clone(), env: self.env.clone() }))
            }
            "ISOMITTED" => {
                let r = matches!(args.first(), Some(Expr::Name(n)) if self.env.iter().rev().find(|(k, _)| gridcraft_formula::same_name(k, n)).is_some_and(|(_, v)| matches!(v, Ev::V(Value::Empty))));
                Ev::V(Value::Bool(r))
            }
            "MAP" | "REDUCE" | "SCAN" | "BYROW" | "BYCOL" | "MAKEARRAY" => self.higher_order(name, args),
            "GROUPBY" | "PIVOTBY" => {
                // The aggregation may be a function given by name (`SUM`), or several stacked
                // (`HSTACK(SUM,MAX)`): pass their names.
                let i = if name == "GROUPBY" { 2 } else { 3 };
                match args.get(i).and_then(|a| self.functions_by_name(a)) {
                    Some(f) => {
                        let mut args = args.to_vec();
                        if let Some(a) = args.get_mut(i) {
                            *a = f;
                        }
                        self.builtin(name, &args)
                    }
                    None => self.builtin(name, args),
                }
            }
            // A LAMBDA that is not called is a value of its own kind (TYPE 128), not an error.
            "TYPE" | "ERROR.TYPE" | "ISERROR" | "ISERR" if args.len() == 1 => match self.eval(&args[0]) {
                Ev::L(_) => match name {
                    "TYPE" => Ev::V(Value::Number(128.0)),
                    "ERROR.TYPE" => err(CellError::NA),
                    _ => Ev::V(Value::Bool(false)),
                },
                ev => self.builtin_evaluated(name, args, ev),
            },
            "NOW" | "TODAY" | "RAND" | "RANDBETWEEN" | "RANDARRAY" => {
                self.host.note_volatile();
                self.builtin(name, args)
            }
            _ => self.builtin(name, args),
        }
    }

    fn if_call(&mut self, args: &[Expr]) -> Ev {
        if args.is_empty() || args.len() > 3 {
            return err(CellError::Value);
        }
        let condition = self.arg_val(args, 0).unwrap_or_default();
        if let Value::Array(array) = &condition {
            let when_true = self.arg_val(args, 1).unwrap_or(Value::Bool(true));
            let when_false = if args.len() > 2 { self.arg_val(args, 2).unwrap_or(Value::Bool(false)) } else { Value::Bool(false) };
            return Ev::V(zip3(array, &when_true, &when_false, |condition, when_true, when_false| match condition.to_bool() {
                Ok(true) => when_true.clone(),
                Ok(false) => when_false.clone(),
                Err(error) => Value::Error(error),
            }));
        }
        match condition.to_bool() {
            Ok(true) => match args.get(1) {
                Some(Expr::Missing) => Ev::V(Value::Empty),
                Some(expression) => self.eval(expression),
                None => Ev::V(Value::Bool(true)),
            },
            Ok(false) => match args.get(2) {
                Some(Expr::Missing) => Ev::V(Value::Empty),
                Some(expression) => self.eval(expression),
                None => Ev::V(Value::Bool(false)),
            },
            Err(error) => err(error),
        }
    }

    fn if_error_call(&mut self, name: &str, args: &[Expr]) -> Ev {
        if args.len() != 2 {
            return err(CellError::Value);
        }
        let value = self.arg_ev(args, 0).unwrap_or(Ev::V(Value::Empty));
        let na_only = name == "IFNA";
        let catches = |error: CellError| !na_only || error == CellError::NA;
        match &value {
            Ev::V(Value::Error(error)) if catches(*error) => self.eval(&args[1]),
            Ev::V(Value::Array(array)) if array.data.iter().any(|item| item.as_error().is_some_and(catches)) => {
                let alternate = self.arg_val(args, 1).unwrap_or_default();
                let data =
                    array.data.iter().map(|item| if item.as_error().is_some_and(catches) { alternate.scalar() } else { item.clone() }).collect();
                Ev::V(Array::new(array.rows, array.cols, data).map(Value::from).unwrap_or(Value::Error(CellError::Value)))
            }
            Ev::R(_) => self.if_error_reference(value.clone(), args, catches),
            _ => value,
        }
    }

    fn if_error_reference(&mut self, value: Ev, args: &[Expr], catches: impl Fn(CellError) -> bool + Copy) -> Ev {
        match self.deref(value.clone()) {
            Value::Error(error) if catches(error) => self.eval(&args[1]),
            Value::Array(ref array) if array.data.iter().any(|item| item.as_error().is_some_and(catches)) => {
                let alternate = self.arg_val(args, 1).unwrap_or_default();
                let data =
                    array.data.iter().map(|item| if item.as_error().is_some_and(catches) { alternate.scalar() } else { item.clone() }).collect();
                Ev::V(Array::new(array.rows, array.cols, data).map(Value::from).unwrap_or(Value::Error(CellError::Value)))
            }
            _ => value,
        }
    }

    fn choose_call(&mut self, args: &[Expr]) -> Ev {
        if args.len() < 2 {
            return err(CellError::Value);
        }
        let index = self.value(&args[0]);
        if let Value::Array(_) = index {
            let Some(spec) = gridcraft_functions::lookup("CHOOSE") else { return err(CellError::Name) };
            let mut evaluated = vec![Arg::val(index)];
            for argument in &args[1..] {
                evaluated.push(Arg::val(self.value(argument)));
            }
            let mut context = FnCtx { ev: self, criteria: None, lambda: None };
            return gridcraft_functions::call(spec, &evaluated, &mut context).pipe(Ev::V);
        }
        let index = match index.to_number() {
            Ok(number) => number.trunc() as i64,
            Err(error) => return err(error),
        };
        if index < 1 || index as usize >= args.len() {
            return err(CellError::Value);
        }
        self.eval(&args[index as usize])
    }

    fn switch_call(&mut self, args: &[Expr]) -> Ev {
        if args.len() < 3 {
            return err(CellError::Value);
        }
        let value = self.arg_val(args, 0).unwrap_or_default().scalar();
        if let Value::Error(error) = value {
            return err(error);
        }
        let choices = &args[1..];
        for pair in choices.as_chunks::<2>().0 {
            let (candidate, result) = (&pair[0], &pair[1]);
            let candidate = self.value(candidate).scalar();
            if !candidate.is_error() && compare(&value, &candidate) == std::cmp::Ordering::Equal && same_kind(&value, &candidate) {
                return self.eval(result);
            }
        }
        if let Some(default) = choices.get(choices.len().saturating_sub(1)).filter(|_| choices.len() % 2 == 1) {
            self.eval(default)
        } else {
            err(CellError::NA)
        }
    }

    /// `T(value)`: text as it is, anything else empty text. A reference to more than one cell is
    /// read at its top-left cell, not by implicit intersection (`T(A1:C1)` in row 5 is `""` when
    /// A1 is a number, where `@A1:C1` would be `#VALUE!`).
    fn t_call(&mut self, args: &[Expr]) -> Ev {
        if args.len() != 1 {
            return err(CellError::Value);
        }
        let value = match self.eval(&args[0]) {
            Ev::R(areas) => match areas.as_slice() {
                [area] if !area.range.is_single() => self.host.cell_value(area.sheet, area.range.start),
                _ => self.deref(Ev::R(areas)).scalar(),
            },
            ev => self.deref(ev).scalar(),
        };
        match value {
            Value::Text(t) => Ev::V(Value::Text(t)),
            Value::Error(e) => err(e),
            _ => Ev::V(Value::text("")),
        }
    }

    fn row_column_call(&mut self, name: &str, args: &[Expr]) -> Ev {
        let is_row = name == "ROW";
        if args.is_empty() || matches!(args.first(), Some(Expr::Missing)) {
            return Ev::V(Value::number(if is_row { self.at.row + 1 } else { self.at.col + 1 } as f64));
        }
        let areas = match self.eval(&args[0]) {
            Ev::R(areas) => areas,
            Ev::V(Value::Error(error)) => return err(error),
            _ => return err(CellError::Value),
        };
        let Some(area) = areas.first() else { return err(CellError::Ref) };
        let range = self.trim_lines(area, is_row);
        if is_row {
            let values: Vec<Value> = (range.start.row..=range.end.row).map(|row| Value::number(row as f64 + 1.0)).collect();
            if values.len() == 1 { Ev::V(values.into_iter().next().unwrap_or_default()) } else { Ev::V(Array::column(values).into()) }
        } else {
            let values: Vec<Value> = (range.start.col..=range.end.col).map(|col| Value::number(col as f64 + 1.0)).collect();
            if values.len() == 1 { Ev::V(values.into_iter().next().unwrap_or_default()) } else { Ev::V(Array::row(values).into()) }
        }
    }

    fn sheet_call(&mut self, name: &str, args: &[Expr]) -> Ev {
        let is_sheet = name == "SHEET";
        if args.is_empty() {
            let count = if is_sheet { self.sheet + 1 } else { self.host.workbook().sheets.len() };
            return Ev::V(Value::number(count as f64));
        }
        match self.eval(&args[0]) {
            Ev::R(areas) if is_sheet => match areas.first() {
                // A sheet of a linked workbook has no number in this one.
                Some(area) if linked_parts(area.sheet).is_some() => err(CellError::NA),
                Some(area) => Ev::V(Value::number(area.sheet as f64 + 1.0)),
                None => err(CellError::Ref),
            },
            Ev::R(areas) => {
                let mut sheets: Vec<usize> = areas.iter().map(|area| area.sheet).collect();
                sheets.dedup();
                Ev::V(Value::number(sheets.len() as f64))
            }
            Ev::V(Value::Text(name)) if is_sheet => {
                self.host.workbook().sheet_index(&name).map_or(err(CellError::NA), |index| Ev::V(Value::number(index as f64 + 1.0)))
            }
            _ => err(CellError::NA),
        }
    }

    pub(super) fn builtin(&mut self, name: &str, args: &[Expr]) -> Ev {
        let Some(spec) = gridcraft_functions::lookup(name) else {
            // A defined name holding a LAMBDA, or a LET-bound lambda.
            return match self.name(name) {
                Ev::L(l) => self.apply_lambda(&l, args),
                Ev::V(Value::Error(x)) => err(x),
                _ => err(CellError::Name),
            };
        };
        if args.len() < spec.min_args || args.len() > spec.max_args {
            return err(CellError::Value);
        }
        if let Some(kind) = Agg::of(spec.name) {
            return self.aggregate(kind, args);
        }
        if let Some(is_range) = criteria_ranges(spec.name) {
            return self.criteria_call(spec, args, is_range);
        }
        let mut state = ArgState::default();
        let mut evaluated = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let ev = self.eval(a);
            if let Some(refused) = self.refused_reference(spec.name, i, &ev, a) {
                return refused;
            }
            let arg = self.builtin_arg(spec, args.len(), i, ev, matches!(a, Expr::Missing), &mut state);
            evaluated.push(arg);
        }
        let (criteria, lambda) = (state.criteria, state.lambda);
        Ev::V(self.call_builtin(spec, &evaluated, criteria, lambda))
    }

    /// A one-argument worksheet function whose argument `args[0]` is already evaluated to `ev`.
    fn builtin_evaluated(&mut self, name: &str, args: &[Expr], ev: Ev) -> Ev {
        let Some(spec) = gridcraft_functions::lookup(name) else { return err(CellError::Name) };
        if args.len() < spec.min_args || args.len() > spec.max_args {
            return err(CellError::Value);
        }
        let mut state = ArgState::default();
        let missing = matches!(args.first(), Some(Expr::Missing));
        let arg = self.builtin_arg(spec, args.len(), 0, ev, missing, &mut state);
        let (criteria, lambda) = (state.criteria, state.lambda);
        Ev::V(self.call_builtin(spec, &[arg], criteria, lambda))
    }

    /// Range parameters of the *IF(S) family take only references, and not references into a
    /// linked workbook: Excel answers `#VALUE!` (or the cell's error) while the source workbook is
    /// closed. A written error (`#REF!` left by a deleted range, here or in a linked workbook:
    /// `[1]Data!#REF!`) is that error. `None`: the argument is taken.
    fn refused_reference(&mut self, name: &str, i: usize, ev: &Ev, a: &Expr) -> Option<Ev> {
        let linked = matches!(ev, Ev::R(areas) if areas.iter().any(|x| linked_parts(x.sheet).is_some()));
        if !(takes_reference(name, i) && (linked || matches!(a, Expr::Error(_) | Expr::LinkedRefError(_)))) {
            return None;
        }
        Some(err(match self.deref(ev.clone()) {
            Value::Error(e) => e,
            _ => CellError::Value,
        }))
    }

    /// One evaluated argument of a worksheet function, read into the value the function gets.
    fn builtin_arg(&mut self, spec: &gridcraft_functions::FnSpec, n_args: usize, i: usize, mut ev: Ev, missing: bool, state: &mut ArgState) -> Arg {
        let name = spec.name;
        let lambda_at = match name {
            "GROUPBY" => Some(2),
            "PIVOTBY" => Some(3),
            _ => None,
        };
        if lambda_at == Some(i)
            && let Ev::L(l) = &ev
        {
            state.lambda = Some(l.clone());
            ev = Ev::V(Value::Empty);
        }
        if spec.category == gridcraft_functions::Category::Database
            && i == 2
            && let Ev::R(areas) = &ev
            && let [area] = areas.as_slice()
        {
            state.criteria = Some(*area);
        }
        if resizes_sum_range(name, n_args) {
            if i == 0 {
                state.criteria_shape = ev_shape(&ev);
            } else if i == 2
                && let (Some((h, w)), Ev::R(areas)) = (state.criteria_shape, &ev)
                && let [area] = areas.as_slice()
            {
                ev = Ev::R(vec![Area { sheet: area.sheet, range: resized_sum_range(area.range, h, w) }]);
            }
        }
        let from_ref = matches!(ev, Ev::R(_));
        let value = match ev {
            Ev::R(areas) if areas.len() > 1 => {
                // Multi-area references (unions): flatten into one row for aggregates.
                let mut vals = Vec::new();
                for ar in &areas {
                    match self.area_value(ar) {
                        Value::Array(a) => vals.extend(a.data.iter().cloned()),
                        v => vals.push(v),
                    }
                }
                Value::from(Array::row(vals))
            }
            ev => self.deref(ev),
        };
        // Only an argument left out (`f(a,,b)`) is omitted; one that computes a blank is given.
        let blank_value = !from_ref && value == Value::Empty && lambda_at != Some(i) && !missing;
        Arg { value, from_ref, blank_value }
    }

    /// Calls a worksheet function with its evaluated arguments.
    fn call_builtin(&mut self, spec: &gridcraft_functions::FnSpec, evaluated: &[Arg], criteria: Option<Area>, lambda: Option<Arc<Lambda>>) -> Value {
        // The function may go through every value it is given (a SUM inside each call of a
        // LAMBDA over a large array): the values count against the cell's work.
        let values = evaluated.iter().fold(0u64, |n, a| n.saturating_add(if let Value::Array(x) = &a.value { x.data.len() as u64 } else { 1 }));
        if !self.spend(values) {
            return Value::Error(CellError::Calc);
        }
        let database = criteria.is_some();
        let mut ctx = FnCtx { ev: self, criteria, lambda };
        let v = gridcraft_functions::call(spec, evaluated, &mut ctx);
        // A computed criterion that ran out of work counted as not met: the result leaves
        // records out.
        if database && self.exhausted {
            return Value::Error(CellError::Calc);
        }
        v
    }

    /// SUMIF, COUNTIF, AVERAGEIF and the *IFS functions. A range given as `INDIRECT` of an array
    /// of texts is an array of references: the function is called once per reference and the
    /// results form an array of that shape (several such ranges broadcast together; a position one
    /// of them lacks is `#N/A`, a text that isn't a reference is `#REF!` in its position).
    fn criteria_call(&mut self, spec: &gridcraft_functions::FnSpec, args: &[Expr], is_range: fn(usize) -> bool) -> Ev {
        let mut evs = Vec::with_capacity(args.len());
        let (mut rows, mut cols, mut lifted) = (1, 1, false);
        for (i, a) in args.iter().enumerate() {
            let arg = if is_range(i) { self.ref_arg(a) } else { RefArg::One(self.eval(a)) };
            if let RefArg::Many(r) = &arg {
                (rows, cols) = if lifted { (broadcast_dim(rows, r.rows), broadcast_dim(cols, r.cols)) } else { (r.rows, r.cols) };
                lifted = true;
            }
            evs.push(arg);
        }
        if !lifted {
            let mut state = ArgState::default();
            let mut evaluated = Vec::with_capacity(evs.len());
            for (i, arg) in evs.into_iter().enumerate() {
                let RefArg::One(ev) = arg else { return err(CellError::Value) };
                if let Some(a) = args.get(i)
                    && let Some(refused) = self.refused_reference(spec.name, i, &ev, a)
                {
                    return refused;
                }
                let missing = args.get(i).is_some_and(|a| matches!(a, Expr::Missing));
                let arg = self.builtin_arg(spec, args.len(), i, ev, missing, &mut state);
                evaluated.push(arg);
            }
            return Ev::V(self.call_builtin(spec, &evaluated, None, None));
        }
        let Some(n) = broadcast_len(rows, cols) else { return err(CellError::Num) };
        let mut out = Vec::with_capacity(n);
        for r in 0..rows {
            for c in 0..cols {
                let mut state = ArgState::default();
                let mut evaluated = Vec::with_capacity(evs.len());
                let mut failed = None;
                for (i, arg) in evs.iter().enumerate() {
                    let ev = match arg {
                        RefArg::One(ev) => ev.clone(),
                        RefArg::Many(m) => {
                            let ev = m.get_broadcast(r, c);
                            if let Ev::V(Value::Error(e)) = ev {
                                failed = Some(e);
                                break;
                            }
                            ev
                        }
                    };
                    let missing = args.get(i).is_some_and(|a| matches!(a, Expr::Missing));
                    let arg = self.builtin_arg(spec, args.len(), i, ev, missing, &mut state);
                    evaluated.push(arg);
                }
                out.push(match failed {
                    Some(e) => Value::Error(e),
                    None => self.call_builtin(spec, &evaluated, None, None),
                });
            }
        }
        Array::new(rows, cols, out).map_or(err(CellError::Value), |a| Ev::V(Value::from(a)))
    }

    /// Performs aggregation operations without copying its ranges: blocks are folded over the stored
    /// cells, so no array is built and there is no `MAX_CELLS` limit. Arguments are evaluated and read one
    /// after another, as the array path does, so volatile formulas (like `RAND`) see the same order.
    /// Only `SUM`, `COUNT` and `AVG` are supported currently.
    fn aggregate(&mut self, kind: Agg, args: &[Expr]) -> Ev {
        let mut acc = Acc::default();
        // An error of a computed argument (`-A1`, `1/0`) wins over one found inside a reference,
        // as in `gridcraft_functions::computed_error`; otherwise the first error in order.
        let (mut ref_error, mut computed_error) = (None, None);
        for a in args {
            let ev = self.eval(a);
            let (steps, slot) = match ev {
                Ev::R(areas) => (areas.iter().map(|area| self.aggregate_area(kind, area, &mut acc)).collect::<Vec<_>>(), &mut ref_error),
                ev => {
                    let value = self.deref(ev);
                    (vec![self.aggregate_value(kind, value, false, &mut acc)], &mut computed_error)
                }
            };
            if slot.is_none() {
                *slot = steps.into_iter().find_map(Result::err);
            }
        }
        computed_error.or(ref_error).unwrap_or_else(|| Ev::V(acc.finish(kind)))
    }

    /// Adds one area to `acc`; `Err` is the result the function ends with.
    fn aggregate_area(&mut self, kind: Agg, a: &Area, acc: &mut Acc) -> Result<(), Ev> {
        if !a.range.is_single() && linked_parts(a.sheet).is_none() {
            // Adding the block's remembered sum to anything but +0.0 would add in another order.
            // COUNT never reads the sum, so it may reuse every block.
            if (kind == Agg::Count || acc.sum.to_bits() == 0)
                && let Some((sum, count)) = self.host.range_sum(a.sheet, a.range)
            {
                acc.sum = sum;
                acc.count = acc.count.saturating_add(count);
                return Ok(());
            }
            // Folded without building an array, but still walked: the part inside the used
            // range counts against the cell's work.
            let r = self.read_range(a);
            if !self.spend(u64::from(r.height()) * u64::from(r.width())) {
                return Err(err(CellError::Calc));
            }
            let mut error = None;
            let walked = self.host.visit_range(a.sheet, a.range, &mut |v| match acc.add_cell(kind, v) {
                Ok(()) => true,
                Err(e) => {
                    error = Some(e);
                    false
                }
            });
            if walked {
                return error.map_or(Ok(()), |e| Err(err(e)));
            }
        }
        let value = self.area_value(a);
        self.aggregate_value(kind, value, true, acc)
    }

    /// Adds one argument's value by calling `SUM` and `COUNT` on it, so coercion and error rules
    /// stay in one place (`AVERAGE` of an argument is its `SUM` over its `COUNT`).
    fn aggregate_value(&mut self, kind: Agg, value: Value, from_ref: bool, acc: &mut Acc) -> Result<(), Ev> {
        if !from_ref && !self.spend_on(&value) {
            return Err(err(CellError::Calc));
        }
        let (Some(sum), Some(count)) = (gridcraft_functions::lookup("SUM"), gridcraft_functions::lookup("COUNT")) else {
            return Err(err(CellError::Name));
        };
        let args = [Arg { value, from_ref, blank_value: false }];
        if kind != Agg::Count {
            match gridcraft_functions::call(sum, &args, &mut FnCtx { ev: self, criteria: None, lambda: None }) {
                Value::Number(n) => acc.sum += n,
                other => return Err(Ev::V(other)),
            }
        }
        if kind != Agg::Sum {
            match gridcraft_functions::call(count, &args, &mut FnCtx { ev: self, criteria: None, lambda: None }) {
                Value::Number(n) => acc.count = acc.count.saturating_add(n as u64),
                other => return Err(Ev::V(other)),
            }
        }
        Ok(())
    }

    /// A worksheet function given by name where a function is expected (`BYROW(A1:C3,SUM)`,
    /// `GROUPBY(A2:A9,B2:B9,SUM)`): its name, unless a LET/LAMBDA variable or a defined name has
    /// that name.
    /// GROUPBY/PIVOTBY's function argument given by name: one function (as its name), or
    /// `HSTACK`/`VSTACK` of functions given by name (as an array of their names, in order).
    fn functions_by_name(&self, e: &Expr) -> Option<Expr> {
        if let Some(f) = self.function_by_name(e) {
            return Some(Expr::Text(f.into()));
        }
        let Expr::Call(n, items) = e else { return None };
        let n = n.to_ascii_uppercase();
        let n = n.strip_prefix("_XLFN.").unwrap_or(&n);
        if !matches!(n, "HSTACK" | "VSTACK") || items.is_empty() {
            return None;
        }
        let mut names = Vec::with_capacity(items.len());
        for item in items {
            names.push(Expr::Text(self.function_by_name(item)?.into()));
        }
        Some(Expr::Array(if n == "HSTACK" { vec![names] } else { names.into_iter().map(|x| vec![x]).collect() }))
    }

    pub(super) fn function_by_name(&self, e: &Expr) -> Option<String> {
        let Expr::Name(n) = e else { return None };
        if self.env.iter().any(|(k, _)| gridcraft_formula::same_name(k, n)) || self.host.workbook().name(n, self.sheet).is_some() {
            return None;
        }
        let u = n.to_ascii_uppercase();
        gridcraft_functions::lookup(&u).map(|_| u)
    }
}

/// What `builtin_arg` carries from one argument to the next.
#[derive(Default)]
struct ArgState {
    /// SUMIF/AVERAGEIF: the size of the criteria range (argument 1), which the sum range takes.
    criteria_shape: Option<(u32, u32)>,
    /// Database functions: the criteria range (argument 2), whose formulas are computed criteria.
    criteria: Option<Area>,
    /// GROUPBY/PIVOTBY: the LAMBDA given as their function.
    lambda: Option<Arc<Lambda>>,
}

/// The criteria functions, with which of their arguments are ranges (references).
fn criteria_ranges(name: &str) -> Option<fn(usize) -> bool> {
    Some(match name {
        "SUMIF" | "AVERAGEIF" => |i| i == 0 || i == 2,
        "COUNTIF" => |i| i == 0,
        "SUMIFS" | "AVERAGEIFS" => |i| i == 0 || i % 2 == 1,
        "COUNTIFS" => |i| i % 2 == 0,
        _ => return None,
    })
}
