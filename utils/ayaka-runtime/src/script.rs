//! The script interpreter.

use crate::plugin::Runtime;
use ayaka_bindings_types::VarMap;
use ayaka_script::*;
use log::warn;
use trylog::TryLog;

/// The variable table in scripts.
pub struct VarTable<'a> {
    /// The plugin runtime.
    pub runtime: &'a Runtime,
    /// The context variables.
    pub locals: &'a mut VarMap,
    /// The locale variables.
    pub vars: VarMap,
}

impl<'a> VarTable<'a> {
    /// Creates a new [`VarTable`].
    pub fn new(runtime: &'a Runtime, locals: &'a mut VarMap) -> Self {
        Self {
            runtime,
            locals,
            vars: VarMap::default(),
        }
    }

    /// Calls a [`Callable`] object.
    pub fn call(&mut self, c: &impl Callable) -> RawValue {
        c.call(self)
    }
}

/// Represents a callable part of a script.
pub trait Callable {
    /// Calls the part with the [`VarTable`].
    fn call(&self, ctx: &mut VarTable) -> RawValue;
}

impl<T: Callable> Callable for &T {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        (*self).call(ctx)
    }
}

impl<T: Callable> Callable for Option<T> {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        match self {
            Some(c) => c.call(ctx),
            None => RawValue::Unit,
        }
    }
}

impl Callable for Program {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        ctx.vars.clear();
        let mut res = RawValue::Unit;
        for expr in &self.0 {
            res = expr.call(ctx);
        }
        res
    }
}

impl Callable for Expr {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        match self {
            Self::Ref(r) => r.call(ctx),
            Self::Const(c) => c.clone(),
            Self::Unary(op, e) => match op {
                UnaryOp::Positive => RawValue::Num(e.call(ctx).get_num()),
                UnaryOp::Negative => RawValue::Num(-e.call(ctx).get_num()),
                UnaryOp::Not => match e.call(ctx) {
                    RawValue::Unit => RawValue::Unit,
                    RawValue::Bool(b) => RawValue::Bool(!b),
                    RawValue::Num(i) => RawValue::Num(!i),
                    RawValue::Str(_) => RawValue::Str(String::new()),
                },
            },
            Self::Binary(lhs, op, rhs) => match op {
                BinaryOp::Val(op) => bin_val(ctx, lhs, op, rhs),
                BinaryOp::Logic(op) => bin_logic(ctx, lhs, op, rhs),
                BinaryOp::Assign => {
                    let val = rhs.call(ctx);
                    assign(ctx, lhs, val)
                }
                BinaryOp::Inplace(op) => {
                    let val = bin_val(ctx, lhs, op, rhs);
                    assign(ctx, lhs, val)
                }
            },
            Self::Call(ns, name, args) => call(ctx, ns, name, args),
        }
    }
}

fn bin_val(ctx: &mut VarTable, lhs: &Expr, op: &ValBinaryOp, rhs: &Expr) -> RawValue {
    let lhs = lhs.call(ctx);
    let rhs = rhs.call(ctx);
    let t = lhs.get_type().max(rhs.get_type());
    match t {
        ValueType::Unit => RawValue::Unit,
        ValueType::Bool => bin_bool_val(lhs.get_bool(), op, rhs.get_bool()),
        ValueType::Num => RawValue::Num(bin_num_val(lhs.get_num(), op, rhs.get_num())),
        ValueType::Str => bin_str_val(lhs, op, rhs),
    }
}

fn bin_bool_val(lhs: bool, op: &ValBinaryOp, rhs: bool) -> RawValue {
    match op {
        ValBinaryOp::Add
        | ValBinaryOp::Minus
        | ValBinaryOp::Mul
        | ValBinaryOp::Div
        | ValBinaryOp::Mod => RawValue::Num(bin_num_val(lhs as i64, op, rhs as i64)),
        ValBinaryOp::And => RawValue::Bool(lhs && rhs),
        ValBinaryOp::Or => RawValue::Bool(lhs || rhs),
        ValBinaryOp::Xor => RawValue::Bool(lhs ^ rhs),
    }
}

fn bin_num_val(lhs: i64, op: &ValBinaryOp, rhs: i64) -> i64 {
    match op {
        ValBinaryOp::Add => lhs + rhs,
        ValBinaryOp::Minus => lhs - rhs,
        ValBinaryOp::Mul => lhs * rhs,
        ValBinaryOp::Div => lhs / rhs,
        ValBinaryOp::Mod => lhs % rhs,
        ValBinaryOp::And => lhs & rhs,
        ValBinaryOp::Or => lhs | rhs,
        ValBinaryOp::Xor => lhs ^ rhs,
    }
}

fn bin_str_val(lhs: RawValue, op: &ValBinaryOp, rhs: RawValue) -> RawValue {
    match op {
        ValBinaryOp::Add => RawValue::Str((lhs.get_str() + rhs.get_str()).into()),
        ValBinaryOp::Mul => match (
            lhs.get_type().max(ValueType::Num),
            rhs.get_type().max(ValueType::Num),
        ) {
            (ValueType::Str, ValueType::Str) => unimplemented!(),
            (ValueType::Num, ValueType::Str) => {
                RawValue::Str(rhs.get_str().repeat(lhs.get_num() as usize))
            }
            (ValueType::Str, ValueType::Num) => {
                RawValue::Str(lhs.get_str().repeat(rhs.get_num() as usize))
            }
            _ => unreachable!(),
        },
        _ => unimplemented!(),
    }
}

fn bin_logic(ctx: &mut VarTable, lhs: &Expr, op: &LogicBinaryOp, rhs: &Expr) -> RawValue {
    let res = match op {
        LogicBinaryOp::And => lhs.call(ctx).get_bool() && rhs.call(ctx).get_bool(),
        LogicBinaryOp::Or => lhs.call(ctx).get_bool() || rhs.call(ctx).get_bool(),
        op => {
            let lhs = lhs.call(ctx);
            let rhs = rhs.call(ctx);
            let t = lhs.get_type().max(rhs.get_type());
            match t {
                ValueType::Unit => false,
                ValueType::Bool => bin_ord_logic(&lhs.get_bool(), op, &rhs.get_bool()),
                ValueType::Num => bin_ord_logic(&lhs.get_num(), op, &rhs.get_num()),
                ValueType::Str => bin_ord_logic(&lhs.get_str(), op, &rhs.get_str()),
            }
        }
    };
    RawValue::Bool(res)
}

fn bin_ord_logic<T: Ord>(lhs: &T, op: &LogicBinaryOp, rhs: &T) -> bool {
    match op {
        LogicBinaryOp::Eq => lhs == rhs,
        LogicBinaryOp::Neq => lhs != rhs,
        LogicBinaryOp::Lt => lhs < rhs,
        LogicBinaryOp::Le => lhs <= rhs,
        LogicBinaryOp::Gt => lhs > rhs,
        LogicBinaryOp::Ge => lhs >= rhs,
        _ => unreachable!(),
    }
}

fn assign(ctx: &mut VarTable, e: &Expr, val: RawValue) -> RawValue {
    match e {
        Expr::Ref(r) => match r {
            Ref::Var(n) => ctx.vars.insert(n.into(), val),
            Ref::Ctx(n) => ctx.locals.insert(n.into(), val),
        },
        _ => unreachable!(),
    };
    RawValue::Unit
}

fn call(ctx: &mut VarTable, ns: &str, name: &str, args: &[Expr]) -> RawValue {
    if ns.is_empty() {
        match name {
            "if" => if args.get(0).call(ctx).get_bool() {
                args.get(1)
            } else {
                args.get(2)
            }
            .call(ctx),
            _ => unimplemented!("intrinstics"),
        }
    } else {
        let args = args.iter().map(|e| e.call(ctx)).collect::<Vec<_>>();
        ctx.runtime
            .module(ns)
            .map(|runtime| {
                runtime
                    .dispatch_method(name, &args)
                    .unwrap_or_default_log_with(|| format!("Calling `{}.{}` error", ns, name))
            })
            .unwrap_or_default_log_with(|| format!("Cannot find namespace `{}`", ns))
    }
}

impl Callable for Ref {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        match self {
            Self::Var(n) => ctx
                .vars
                .get(n)
                .cloned()
                .unwrap_or_default_log("Cannot find variable"),
            Self::Ctx(n) => ctx
                .locals
                .get(n)
                .cloned()
                .unwrap_or_default_log("Cannot find context variable"),
        }
    }
}

impl Callable for Text {
    fn call(&self, ctx: &mut VarTable) -> RawValue {
        let mut str = String::new();
        for line in &self.0 {
            match line {
                SubText::Str(s) => str.push_str(s),
                SubText::Cmd(c) => {
                    let value = match c {
                        Command::Character(_, _) => RawValue::Unit,
                        Command::Res(_) | Command::Other(_, _) => {
                            warn!("Unsupported command in text.");
                            RawValue::Unit
                        }
                        Command::Ctx(n) => ctx
                            .locals
                            .get(n)
                            .cloned()
                            .unwrap_or_default_log("Cannot find variable"),
                    };
                    str.push_str(&value.get_str());
                }
            }
        }
        RawValue::Str(str.trim().to_string())
    }
}

#[cfg(test)]
mod test {
    use crate::{plugin::Runtime, script::*};
    use tokio::sync::{Mutex, OnceCell};

    static RUNTIME: OnceCell<Mutex<Runtime>> = OnceCell::const_new();

    async fn with_ctx(f: impl FnOnce(&mut VarTable)) {
        let runtime = RUNTIME
            .get_or_init(|| async {
                let runtime = Runtime::load(
                    "../../examples/plugins",
                    env!("CARGO_MANIFEST_DIR"),
                    &["random"],
                );
                Mutex::new(runtime.await.unwrap())
            })
            .await;
        let runtime = runtime.lock().await;
        let mut locals = VarMap::default();
        let mut ctx = VarTable::new(&runtime, &mut locals);
        f(&mut ctx);
    }

    #[tokio::test]
    async fn vars() {
        with_ctx(|ctx| {
            assert_eq!(
                ProgramParser::new()
                    .parse(
                        "
                            a = 0;
                            a += 1;
                            a += a;
                            a
                        "
                    )
                    .ok()
                    .call(ctx),
                RawValue::Num(2)
            );

            assert_eq!(
                ProgramParser::new().parse("a").ok().call(ctx),
                RawValue::Unit
            );

            assert_eq!(
                ProgramParser::new()
                    .parse(
                        "
                            $a = 0;
                            $a += 1;
                            $a += a;
                            $a
                        "
                    )
                    .ok()
                    .call(ctx),
                RawValue::Num(1)
            );

            assert_eq!(
                ProgramParser::new().parse("$a").ok().call(ctx),
                RawValue::Num(1)
            );
        })
        .await;
    }

    #[tokio::test]
    async fn if_test() {
        with_ctx(|ctx| {
            assert_eq!(
                ProgramParser::new()
                    .parse(
                        r##"
                            if(1 + 1 + 4 + 5 + 1 + 4 == 16, "sodayo", ~)
                        "##
                    )
                    .ok()
                    .call(ctx)
                    .get_num(),
                6
            );
            assert_eq!(
                ProgramParser::new()
                    .parse(
                        r##"
                            if(true, "sodayo")
                        "##
                    )
                    .ok()
                    .call(ctx)
                    .get_str(),
                "sodayo"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn random() {
        with_ctx(|ctx| {
            assert!((0..10).contains(
                &ProgramParser::new()
                    .parse(
                        r##"
                            random.rnd(10)
                        "##
                    )
                    .ok()
                    .call(ctx)
                    .get_num()
            ))
        })
        .await;
    }
}
