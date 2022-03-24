use crate::*;
use gal_script::*;

pub trait Callable {
    fn call(&self, ctx: &mut Context<'_>) -> Option<Value>;
}

impl Callable for Program {
    fn call(&self, ctx: &mut Context<'_>) -> Option<Value> {
        ctx.locals.clear();
        let mut res = None;
        for expr in &self.0 {
            res = expr.call(ctx);
        }
        res
    }
}

impl Callable for Expr {
    fn call(&self, ctx: &mut Context<'_>) -> Option<Value> {
        match self {
            Self::Ref(r) => r.call(ctx),
            Self::Const(c) => c.call(ctx),
            Self::Call(_, _) => unimplemented!("Call functions"),
        }
    }
}

impl Callable for Ref {
    fn call(&self, ctx: &mut Context<'_>) -> Option<Value> {
        match self {
            Self::Var(n) => ctx.ctx.vars.get(n).map(|v| v.clone()),
            Self::Ctx(n) => ctx.locals.get(n).map(|v| v.clone()),
            Self::Res(_) => unimplemented!("Resources"),
        }
    }
}

impl Callable for Const {
    fn call(&self, _ctx: &mut Context<'_>) -> Option<Value> {
        match self {
            Self::Bool(b) => Some(Value::Bool(*b)),
            Self::Num(n) => Some(Value::Num(*n)),
            Self::Str(s) => Some(Value::Str(s.clone())),
        }
    }
}

pub trait Evaluable {
    fn eval(&self, ctx: &mut Context<'_>) -> Value;
    fn eval_str(&self, ctx: &mut Context<'_>) -> String {
        match self.eval(ctx) {
            Value::Bool(b) => b.to_string(),
            Value::Num(i) => i.to_string(),
            Value::Str(s) => s.clone(),
            Value::Expr(_) => unreachable!(),
        }
    }
}

impl Evaluable for Value {
    fn eval(&self, ctx: &mut Context<'_>) -> Value {
        match self {
            Value::Expr(p) => p.call(ctx).unwrap_or(Value::Str(String::new())),
            _ => self.clone(),
        }
    }
}
