use std::collections::HashMap;
use std::fmt::Write;

use crate::term::*;

// Entry point

pub fn codegen(code: &[Item], bindings: &mut Bindings, out_class: &str) -> String {
    let mut cxt = Cxt::new(bindings);
    // Declare items
    let mut mappings = Vec::new();
    let mut java = Vec::new();
    let mut enums = Vec::new();
    for i in code {
        let (name, ret, m, public) = match i {
            Item::Fn(f) => (f.id, &f.ret_ty, cxt.bindings.fn_name(f.id), f.public),
            Item::ExternFn(f) => (f.id, &f.ret_ty, f.mapping, true),
            Item::ExternClass(c) => {
                let class = cxt.fresh_class();
                cxt.types.push((*c, class));
                mappings.push((class.0, cxt.bindings.type_name(*c), false));

                continue;
            }
            Item::Enum(c, variants, ext) => {
                let class = cxt.fresh_class();
                cxt.types.push((*c, class));
                mappings.push((class.0, cxt.bindings.type_name(*c), false));

                if !ext {
                    enums.push((class, variants));
                }

                continue;
            }
            Item::InlineJava(s) => {
                java.push(*s);
                continue;
            }
        };
        let item = cxt.fresh_fn();
        cxt.fn_ids.push((name, item));

        let ret = ret.lower(&cxt);
        cxt.fn_ret_tys.insert(item, ret);
        mappings.push((item.0, m, !public));
    }
    for i in code {
        i.lower(&mut cxt);
    }

    let fns = cxt.items;
    let mut gen = Gen::new(bindings);
    // Declare items
    for (i, m, b) in mappings {
        gen.names.insert(i, (m, b));
    }
    // Generate items
    let mut s = String::new();
    // Add module-level inline Java at the top
    for i in java {
        s.push_str(bindings.resolve_raw(i));
        s.push('\n');
    }
    write!(s, "\npublic class {} {{\n\n", out_class).unwrap();
    for f in fns {
        s.push_str(&f.gen(&mut gen));
    }
    s.push_str("\n}");

    s
}

// Java AST

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// bool: whether it's public, so mangling should be skipped
struct JVar(u64, bool);
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct JFnId(u64);
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct JClass(u64);
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct JBlock(u64);

#[derive(Copy, Clone, Debug, PartialEq)]
enum JLit {
    Int(i32),
    Long(i64),
    Str(RawSym),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
enum JTerm {
    Var(JVar, JTy),
    Lit(JLit),
    Call(Option<Box<JTerm>>, JFnId, Vec<JTerm>, JTy),
    BinOp(BinOp, Box<JTerm>, Box<JTerm>),
    Variant(JClass, RawSym),
    Array(Vec<JTerm>, JTy),
    Index(Box<JTerm>, Box<JTerm>, JTy),
}

#[derive(Clone, Debug, PartialEq)]
enum JStmt {
    Let(RawSym, JTy, JVar, Option<JTerm>),
    Set(JVar, JTerm),
    Term(JTerm),
    If(JTerm, Vec<JStmt>, Vec<JStmt>),
    Switch(JBlock, JTerm, Vec<(RawSym, Vec<JStmt>)>, Vec<JStmt>),
    While(JBlock, JTerm, Vec<JStmt>),
    RangeFor(JBlock, RawSym, JVar, JTerm, JTerm, Vec<JStmt>),
    Continue(JBlock),
    Break(JBlock),
    Ret(JFnId, Vec<JTerm>),
    MultiCall(
        Option<Box<JTerm>>,
        JFnId,
        Vec<JTerm>,
        Vec<(RawSym, JVar, JTy)>,
    ),
    InlineJava(RawSym),
}

#[derive(Clone, Debug, PartialEq)]
enum JTy {
    I32,
    I64,
    Bool,
    String,
    Class(JClass),
    Array(Box<JTy>),
}
impl JTy {
    fn primitive(&self) -> bool {
        match self {
            JTy::I32 => true,
            JTy::I64 => true,
            JTy::Bool => true,
            JTy::String => false,
            JTy::Class(_) => false,
            JTy::Array(_) => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct JFn {
    name: RawSym,
    fn_id: JFnId,
    ret_tys: Vec<JTy>,
    args: Vec<(RawSym, JVar, JTy)>,
    body: Vec<JStmt>,
    public: bool,
}

/// This only includes the items that actually need to appear in the Java code
/// i.e. not extern things
#[derive(Clone, Debug, PartialEq)]
enum JItem {
    Fn(JFn),
    Enum(JClass, Vec<RawSym>),
}

#[derive(Clone, Debug, PartialEq)]
enum MaybeList<T> {
    One(T),
    Tuple(Vec<T>),
}
impl<T> MaybeList<T> {
    fn one(self) -> T {
        match self {
            MaybeList::One(t) => t,
            MaybeList::Tuple(mut v) => {
                if v.len() == 1 {
                    v.pop().unwrap()
                } else {
                    panic!("backend: one object required, but got {}", v.len())
                }
            }
        }
    }

    fn to_vec(self) -> Vec<T> {
        match self {
            MaybeList::One(t) => vec![t],
            MaybeList::Tuple(v) => v,
        }
    }

    fn is_none(&self) -> bool {
        matches!(self, MaybeList::Tuple(v) if v.is_empty())
    }

    fn len(&self) -> usize {
        match self {
            MaybeList::One(_) => 1,
            MaybeList::Tuple(v) => v.len(),
        }
    }

    fn map<U>(self, mut f: impl FnMut(T) -> U) -> MaybeList<U> {
        match self {
            MaybeList::One(x) => MaybeList::One(f(x)),
            MaybeList::Tuple(v) => MaybeList::Tuple(v.into_iter().map(f).collect()),
        }
    }

    fn empty() -> Self {
        Self::Tuple(Vec::new())
    }
}
impl<T> From<MaybeList<T>> for Vec<T> {
    fn from(j: MaybeList<T>) -> Vec<T> {
        j.to_vec()
    }
}
impl<T> IntoIterator for MaybeList<T> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.to_vec().into_iter()
    }
}

type JTerms = MaybeList<JTerm>;
type JTys = MaybeList<JTy>;
type JVars = MaybeList<JVar>;

impl JTerms {
    fn ty(&self) -> JTys {
        match self {
            MaybeList::One(t) => MaybeList::One(t.ty()),
            MaybeList::Tuple(v) => MaybeList::Tuple(v.iter().map(|x| x.ty()).collect()),
        }
    }
}

// CODEGEN

#[derive(Clone, Debug)]
struct Gen<'a> {
    bindings: &'a Bindings,
    /// The bool is whether to mangle names for deduplication
    names: HashMap<u64, (RawSym, bool)>,
    indent: usize,
}
impl<'a> Gen<'a> {
    fn new(bindings: &'a Bindings) -> Self {
        Gen {
            bindings,
            names: HashMap::new(),
            indent: 0,
        }
    }

    fn push(&mut self) {
        self.indent += 1;
    }
    fn pop(&mut self) {
        self.indent -= 1;
    }
    fn indent(&self) -> &'static str {
        // More than five indentation levels will make it harder to read rather than easier
        let s = "\t\t\t\t\t";
        &s[0..self.indent.min(s.len())]
    }

    fn name_str(&self, v: JVar) -> String {
        let (i, b) = self.names[&v.0];
        let s = self.bindings.resolve_raw(i);
        if b {
            format!("{}${}", s, v.0)
        } else {
            s.to_string()
        }
    }
    fn fn_str(&self, v: JFnId) -> String {
        let (i, b) = self.names[&v.0];
        let s = self.bindings.resolve_raw(i);
        if b {
            format!("{}${}", s, v.0)
        } else {
            s.to_string()
        }
    }
    fn class_str(&self, v: JClass) -> String {
        let (i, b) = self.names[&v.0];
        let s = self.bindings.resolve_raw(i);
        if b {
            format!("{}${}", s, v.0)
        } else {
            s.to_string()
        }
    }
}

impl JTerm {
    fn gen(&self, cxt: &Gen) -> String {
        match self {
            JTerm::Var(v, _) => cxt.name_str(*v),
            JTerm::Lit(l) => match l {
                JLit::Int(i) => i.to_string(),
                JLit::Long(i) => format!("{}L", i),
                JLit::Str(s) => format!("\"{}\"", cxt.bindings.resolve_raw(*s)),
                JLit::Bool(b) => b.to_string(),
            },
            JTerm::Call(None, f, a, _) => {
                let mut buf = String::new();
                buf.push_str(&cxt.fn_str(*f));
                buf.push('(');

                let mut first = true;
                for i in a {
                    if !first {
                        buf.push_str(", ");
                    }
                    first = false;

                    buf.push_str(&i.gen(cxt));
                }
                buf.push(')');

                buf
            }
            JTerm::Call(Some(obj), f, a, _) => {
                let mut buf = format!("({}).", obj.gen(cxt));
                buf.push_str(&cxt.fn_str(*f));
                buf.push('(');

                let mut first = true;
                for i in a {
                    if !first {
                        buf.push_str(", ");
                    }
                    first = false;

                    buf.push_str(&i.gen(cxt));
                }
                buf.push(')');

                buf
            }
            JTerm::BinOp(op @ (BinOp::Eq | BinOp::Neq), a, b) if !a.ty().primitive() => {
                let mut buf = String::new();
                if *op == BinOp::Neq {
                    buf.push('!');
                }
                write!(buf, "({}).equals({})", a.gen(cxt), b.gen(cxt)).unwrap();
                buf
            }
            JTerm::BinOp(op, a, b) => {
                let mut buf = String::new();
                write!(buf, "({}) ", a.gen(cxt)).unwrap();
                buf.push_str(op.repr());
                write!(buf, " ({})", b.gen(cxt)).unwrap();
                buf
            }
            JTerm::Variant(class, variant) => {
                format!(
                    "{}.{}",
                    cxt.bindings.resolve_raw(cxt.names[&class.0].0),
                    cxt.bindings.resolve_raw(*variant)
                )
            }
            JTerm::Array(v, t) => {
                let mut buf = format!("new {}{{ ", t.gen(cxt));
                for i in v {
                    buf.push_str(&i.gen(cxt));
                    buf.push_str(", ");
                }
                buf.push('}');
                buf
            }
            JTerm::Index(arr, i, _) => {
                format!("{}[{}]", arr.gen(cxt), i.gen(cxt))
            }
        }
    }
}
impl JStmt {
    fn gen(&self, cxt: &mut Gen) -> String {
        match self {
            JStmt::Let(n, t, v, None) => {
                cxt.names.insert(v.0, (*n, !v.1));
                format!("{} {} = {};", t.gen(cxt), cxt.name_str(*v), t.null())
            }
            JStmt::Let(n, t, v, Some(x)) => {
                cxt.names.insert(v.0, (*n, !v.1));
                format!("{} {} = {};", t.gen(cxt), cxt.name_str(*v), x.gen(cxt))
            }
            JStmt::Set(v, x) => {
                format!("{} = {};", cxt.name_str(*v), x.gen(cxt))
            }
            JStmt::Term(x) => {
                let mut s = x.gen(cxt);
                s.push(';');
                s
            }
            JStmt::While(k, cond, block) => {
                let mut s = format!("b${}: while ({}) {{", k.0, cond.gen(cxt));
                cxt.push();
                for i in block {
                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push_str(&i.gen(cxt));
                }
                cxt.pop();

                s.push('\n');
                s.push_str(cxt.indent());
                s.push('}');

                s
            }
            JStmt::RangeFor(k, n, var, a, b, block) => {
                cxt.names.insert(var.0, (*n, !var.1));
                let i = cxt.name_str(*var);
                let mut s = format!(
                    "b${}: for (int {} = {}, $end = {}; {} < $end; {}++) {{",
                    k.0,
                    i,
                    a.gen(cxt),
                    b.gen(cxt),
                    i,
                    i
                );

                cxt.push();
                for i in block {
                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push_str(&i.gen(cxt));
                }
                cxt.pop();

                s.push('\n');
                s.push_str(cxt.indent());
                s.push('}');

                s
            }
            JStmt::Continue(k) => format!("continue b${};", k.0),
            JStmt::Break(k) => format!("break b${};", k.0),
            JStmt::Ret(_, v) if v.is_empty() => "return;".into(),
            JStmt::Ret(_, v) if v.len() == 1 => format!("return {};", v[0].gen(cxt)),
            JStmt::Ret(f, v) => {
                let mut s = String::new();

                for (i, t) in v.iter().enumerate() {
                    write!(s, "{}$_ret{}$S = {};", cxt.fn_str(*f), i, t.gen(cxt)).unwrap();
                    s.push('\n');
                    s.push_str(cxt.indent());
                }

                s.push_str("return;");
                s
            }
            JStmt::If(cond, a, b) => {
                let mut s = format!("if ({}) {{", cond.gen(cxt));
                cxt.push();
                for i in a {
                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push_str(&i.gen(cxt));
                }
                cxt.pop();

                s.push('\n');
                s.push_str(cxt.indent());
                s.push('}');

                if !b.is_empty() {
                    cxt.push();

                    s.push_str(" else {");
                    for i in b {
                        s.push('\n');
                        s.push_str(cxt.indent());
                        s.push_str(&i.gen(cxt));
                    }
                    cxt.pop();

                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push('}');
                }

                s
            }
            JStmt::Switch(k, x, branches, default) => {
                let mut s = format!("b${}: switch ({}) {{", k.0, x.gen(cxt));
                for (sym, block) in branches {
                    // case Variant:
                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push_str("case ");
                    s.push_str(cxt.bindings.resolve_raw(*sym));
                    s.push(':');

                    cxt.push();
                    for i in block {
                        s.push('\n');
                        s.push_str(cxt.indent());
                        s.push_str(&i.gen(cxt));
                    }
                    s.push('\n');
                    s.push_str(cxt.indent());
                    write!(s, "break b${};", k.0).unwrap();
                    cxt.pop();
                }

                s.push('\n');
                s.push_str(cxt.indent());
                s.push_str("default:");
                cxt.push();
                for i in default {
                    s.push('\n');
                    s.push_str(cxt.indent());
                    s.push_str(&i.gen(cxt));
                }
                cxt.pop();

                s.push('\n');
                s.push_str(cxt.indent());
                s.push('}');

                s
            }
            JStmt::MultiCall(o, f, args, rets) => {
                let mut buf = o
                    .as_ref()
                    .map(|x| {
                        let mut s = x.gen(cxt);
                        s.push('.');
                        s
                    })
                    .unwrap_or_default();
                buf.push_str(&cxt.fn_str(*f));
                buf.push('(');

                let mut first = true;
                for i in args {
                    if !first {
                        buf.push_str(", ");
                    }
                    first = false;

                    buf.push_str(&i.gen(cxt));
                }
                buf.push_str(");");

                for (i, (raw, v, t)) in rets.iter().enumerate() {
                    cxt.names.insert(v.0, (*raw, !v.1));
                    write!(
                        buf,
                        "\n{}{} {} = {}$_ret{}$S;",
                        cxt.indent(),
                        t.gen(cxt),
                        cxt.name_str(*v),
                        cxt.fn_str(*f),
                        i
                    )
                    .unwrap();
                }

                buf
            }
            JStmt::InlineJava(s) => cxt.bindings.resolve_raw(*s).to_string(),
        }
    }
}
impl JTy {
    fn gen(&self, cxt: &Gen) -> String {
        match self {
            JTy::I32 => "int".into(),
            JTy::I64 => "long".into(),
            JTy::Bool => "boolean".into(),
            JTy::String => "String".into(),
            // Classes are all external, so are never mangled
            JTy::Class(c) => cxt.bindings.resolve_raw(cxt.names[&c.0].0).into(),
            JTy::Array(t) => {
                let mut s = t.gen(cxt);
                s.push_str("[]");
                s
            }
        }
    }
    fn null(&self) -> &'static str {
        match self {
            JTy::I32 => "0",
            JTy::I64 => "0L",
            JTy::Bool => "false",
            JTy::String => "null",
            JTy::Class(_) => "null",
            JTy::Array(_) => "null",
        }
    }
}
impl JFn {
    fn gen(&self, cxt: &mut Gen) -> String {
        let mut buf = String::new();

        if self.ret_tys.len() != 1 {
            // Generate static variables to return tuples into
            // This uses a little bit less bytecode than using e.g. custom classes
            for (i, ty) in self.ret_tys.iter().enumerate() {
                write!(
                    buf,
                    "public static {} {}$_ret{}$S;\n{}",
                    ty.gen(cxt),
                    cxt.fn_str(self.fn_id),
                    i,
                    cxt.indent(),
                )
                .unwrap();
            }
        }

        write!(
            buf,
            "public static {} {}(",
            if self.ret_tys.len() == 1 {
                self.ret_tys[0].gen(cxt)
            } else {
                "void".into()
            },
            cxt.fn_str(self.fn_id)
        )
        .unwrap();
        let names = cxt.names.clone();

        let mut first = true;
        for (n, v, t) in &self.args {
            if !first {
                buf.push_str(", ");
            }
            first = false;
            cxt.names.insert(v.0, (*n, !v.1));
            write!(buf, "{} {}", t.gen(cxt), cxt.name_str(*v),).unwrap();
        }
        buf.push_str(") {");

        cxt.push();

        for i in &self.body {
            buf.push('\n');
            buf.push_str(cxt.indent());
            buf.push_str(&i.gen(cxt));
        }

        cxt.names = names;
        cxt.pop();

        buf.push('\n');
        buf.push_str(cxt.indent());
        buf.push_str("}\n");
        buf.push_str(cxt.indent());
        buf
    }
}
impl JItem {
    fn gen(&self, cxt: &mut Gen) -> String {
        match self {
            JItem::Fn(f) => f.gen(cxt),
            JItem::Enum(tid, variants) => {
                let mut buf = String::new();
                write!(buf, "public static enum {} {{", cxt.class_str(*tid),).unwrap();
                cxt.push();

                for i in variants {
                    buf.push('\n');
                    buf.push_str(cxt.indent());
                    buf.push_str(cxt.bindings.resolve_raw(*i));
                    buf.push(',');
                }

                cxt.pop();
                buf.push('\n');
                buf.push_str(cxt.indent());
                buf.push_str("}\n");
                buf.push_str(cxt.indent());
                buf
            }
        }
    }
}

// LOWERING

#[derive(Debug)]
struct Cxt<'a> {
    bindings: &'a mut Bindings,
    scopes: Vec<(usize, usize, usize)>,
    vars: Vec<(Sym, JVars)>,
    tys: HashMap<JVar, JTy>,
    fn_ids: Vec<(FnId, JFnId)>,
    fn_ret_tys: HashMap<JFnId, JTys>,
    types: Vec<(TypeId, JClass)>,
    block: Vec<JStmt>,
    blocks: Vec<(Option<JBlock>, usize)>,
    current_fn: JFnId,
    items: Vec<JItem>,
    next: u64,
}
impl<'a> Cxt<'a> {
    fn new(bindings: &'a mut Bindings) -> Self {
        Cxt {
            bindings,
            scopes: Vec::new(),
            vars: Vec::new(),
            tys: HashMap::new(),
            fn_ids: Vec::new(),
            fn_ret_tys: HashMap::new(),
            types: Vec::new(),
            block: Vec::new(),
            blocks: Vec::new(),
            current_fn: JFnId(0),
            items: Vec::new(),
            next: 0,
        }
    }

    fn var(&self, s: Sym) -> Option<JVars> {
        self.vars
            .iter()
            .rfind(|(k, _v)| *k == s)
            .map(|(_k, v)| v.clone())
    }
    fn fun(&self, s: FnId) -> Option<JFnId> {
        self.fn_ids
            .iter()
            .rfind(|(k, _v)| *k == s)
            .map(|(_k, v)| *v)
    }
    fn class(&self, s: TypeId) -> Option<JClass> {
        self.types.iter().rfind(|(k, _v)| *k == s).map(|(_k, v)| *v)
    }

    fn block_label(&self) -> Option<JBlock> {
        self.blocks.iter().rev().find_map(|(x, _)| x.clone())
    }
    fn push_loop(&mut self, k: JBlock) {
        self.push();
        self.blocks.push((Some(k), self.block.len()));
    }
    /// Implies push()
    fn push_block(&mut self) {
        self.push();
        self.blocks.push((None, self.block.len()));
    }
    fn pop_block(&mut self) -> Vec<JStmt> {
        self.pop();
        self.block.split_off(self.blocks.pop().unwrap().1)
    }

    fn push(&mut self) {
        self.scopes
            .push((self.vars.len(), self.fn_ids.len(), self.types.len()));
    }
    fn pop(&mut self) {
        let (v, i, t) = self.scopes.pop().unwrap();
        self.vars.truncate(v);
        self.fn_ids.truncate(i);
        self.types.truncate(t);
    }

    fn fresh_var(&mut self, public: bool) -> JVar {
        self.next += 1;
        JVar(self.next, public)
    }
    fn fresh_fn(&mut self) -> JFnId {
        self.next += 1;
        JFnId(self.next)
    }
    fn fresh_class(&mut self) -> JClass {
        self.next += 1;
        JClass(self.next)
    }
    fn fresh_block(&mut self) -> JBlock {
        self.next += 1;
        JBlock(self.next)
    }
}

impl JTerm {
    fn ty(&self) -> JTy {
        match self {
            JTerm::Var(_, t) => t.clone(),
            JTerm::Lit(l) => match l {
                JLit::Int(_) => JTy::I32,
                JLit::Long(_) => JTy::I64,
                JLit::Str(_) => JTy::String,
                JLit::Bool(_) => JTy::Bool,
            },
            JTerm::Call(_, _, _, t) => t.clone(),
            JTerm::Array(_, t) => t.clone(),
            JTerm::Index(_, _, t) => t.clone(),
            JTerm::BinOp(op, a, _) => match op.ty() {
                BinOpType::Comp => JTy::Bool,
                BinOpType::Arith => a.ty(),
                BinOpType::Logic => JTy::Bool,
            },
            JTerm::Variant(class, _) => JTy::Class(*class),
        }
    }
}

impl Term {
    fn lower(&self, cxt: &mut Cxt) -> JTerms {
        JTerms::One(match self {
            Term::Var(s) => {
                let var = cxt.var(*s).unwrap();
                return var.map(|var| JTerm::Var(var, cxt.tys.get(&var).unwrap().clone()));
            }
            Term::Lit(l, t) => match l {
                Literal::Int(i) => match t {
                    Type::I32 => JTerm::Lit(JLit::Int(*i as i32)),
                    Type::I64 => JTerm::Lit(JLit::Long(*i)),
                    _ => unreachable!(),
                },
                Literal::Str(s) => JTerm::Lit(JLit::Str(*s)),
                Literal::Bool(b) => JTerm::Lit(JLit::Bool(*b)),
            },
            Term::Break => {
                cxt.block.push(JStmt::Break(
                    cxt.block_label().expect("'break' outside of loop"),
                ));
                return JTerms::empty();
            }
            Term::Continue => {
                cxt.block.push(JStmt::Continue(
                    cxt.block_label().expect("'continue' outside of loop"),
                ));
                return JTerms::empty();
            }
            Term::Return(x) => {
                let x = x.as_ref().map(|x| x.lower(cxt));
                cxt.block.push(JStmt::Ret(
                    cxt.current_fn,
                    x.into_iter().flatten().collect(),
                ));
                return JTerms::empty();
            }
            Term::Variant(tid, s) => JTerm::Variant(cxt.class(*tid).unwrap(), *s),
            Term::Tuple(v) => return JTerms::Tuple(v.iter().flat_map(|x| x.lower(cxt)).collect()),
            Term::TupleIdx(x, i) => {
                let x = x.lower(cxt);
                x.to_vec().swap_remove(*i)
            }
            Term::Call(o, f, a) => {
                let fn_id = cxt.fun(*f).unwrap();
                let o = o.as_ref().map(|x| Box::new(x.lower(cxt).one()));
                let args = a.iter().flat_map(|x| x.lower(cxt)).collect();
                let rtys = cxt.fn_ret_tys.get(&fn_id).unwrap().clone();
                match rtys {
                    MaybeList::One(rty) => JTerm::Call(o, fn_id, args, rty),
                    MaybeList::Tuple(v) => {
                        // MultiCall time
                        let mut vars = Vec::new();
                        let mut terms = Vec::new();
                        for (i, ty) in v.into_iter().enumerate() {
                            let var = cxt.fresh_var(false);
                            cxt.tys.insert(var, ty.clone());
                            let raw = cxt.bindings.raw(format!(
                                "{}$_call_ret{}",
                                cxt.bindings.resolve_raw(cxt.bindings.fn_name(*f)),
                                i
                            ));
                            terms.push(JTerm::Var(var, ty.clone()));
                            vars.push((raw, var, ty));
                        }
                        cxt.block.push(JStmt::MultiCall(o, fn_id, args, vars));

                        return JTerms::Tuple(terms);
                    }
                }
            }
            Term::BinOp(op, a, b) => JTerm::BinOp(
                *op,
                Box::new(a.lower(cxt).one()),
                Box::new(b.lower(cxt).one()),
            ),
            Term::Block(v, e) => {
                cxt.push();
                for i in v {
                    i.lower(cxt);
                }
                let r = e
                    .as_ref()
                    .map(|x| x.lower(cxt))
                    .unwrap_or(JTerms::Tuple(Vec::new()));
                cxt.pop();
                return r;
            }
            Term::If(cond, a, b) => {
                let cond = cond.lower(cxt).one();

                cxt.push_block();
                let a = a.lower(cxt);
                let ty = a.ty();

                let vars: Vec<_> = ty
                    .clone()
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        (
                            cxt.fresh_var(false),
                            cxt.bindings.raw(format!("_then${}", i)),
                            t,
                        )
                    })
                    .collect();
                for ((var, _, ty), a) in vars.iter().zip(a) {
                    cxt.tys.insert(*var, ty.clone());
                    cxt.block.push(JStmt::Set(*var, a));
                }
                let a = cxt.pop_block();

                let b = if let Some(b) = b {
                    cxt.push_block();
                    let b = b.lower(cxt);
                    for ((var, _, _), b) in vars.iter().zip(b) {
                        cxt.block.push(JStmt::Set(*var, b));
                    }
                    cxt.pop_block()
                } else {
                    Vec::new()
                };

                let mut ret = Vec::new();
                for (var, raw, ty) in vars {
                    cxt.block.push(JStmt::Let(raw, ty.clone(), var, None));
                    ret.push(JTerm::Var(var, ty));
                }
                cxt.block.push(JStmt::If(cond, a, b));

                return JTerms::Tuple(ret);
            }
            Term::Match(x, branches) => {
                let x = x.lower(cxt).one();

                let mut v = Vec::new();
                let mut default = None;
                let mut vars: Option<Vec<_>> = None;
                for (s, t) in branches {
                    cxt.push_block();
                    let t = t.lower(cxt);
                    if vars.is_none() {
                        let ty = t.ty();
                        vars = Some(
                            ty.clone()
                                .into_iter()
                                .enumerate()
                                .map(|(i, t)| {
                                    (
                                        cxt.fresh_var(false),
                                        cxt.bindings.raw(format!("_then${}", i)),
                                        t,
                                    )
                                })
                                .collect(),
                        );
                        for (var, _, ty) in vars.as_ref().unwrap() {
                            cxt.tys.insert(*var, ty.clone());
                        }
                    }
                    for ((var, _, _), t) in vars.as_ref().unwrap().iter().zip(t) {
                        cxt.block.push(JStmt::Set(*var, t));
                    }
                    let block = cxt.pop_block();

                    match s {
                        Some(s) => v.push((*s, block)),
                        None => {
                            if default.is_none() {
                                default = Some(block);
                            } else {
                                unreachable!()
                            }
                        }
                    }
                }

                let mut ret = Vec::new();
                for (var, raw, ty) in vars.unwrap() {
                    cxt.block.push(JStmt::Let(raw, ty.clone(), var, None));
                    ret.push(JTerm::Var(var, ty));
                }
                let k = cxt.fresh_block();
                cxt.block
                    .push(JStmt::Switch(k, x, v, default.unwrap_or_default()));

                return JTerms::Tuple(ret);
            }
        })
    }
}
impl Statement {
    fn lower(&self, cxt: &mut Cxt) {
        match self {
            Statement::Term(x) => {
                let terms = x.lower(cxt);
                for i in terms {
                    cxt.block.push(JStmt::Term(i));
                }
            }
            Statement::Let(n, t, x) => {
                let x = x.lower(cxt);
                let t = t.lower(cxt);
                let mut vars = Vec::new();

                for (x, t) in x.into_iter().zip(t) {
                    let var = cxt.fresh_var(cxt.bindings.public(*n));
                    cxt.tys.insert(var, t.clone());
                    cxt.block.push(JStmt::Let(n.raw(), t, var, Some(x)));
                    vars.push(var);
                }

                cxt.vars.push((*n, JVars::Tuple(vars)));
            }
            Statement::While(cond, block) => {
                let cond = cond.lower(cxt).one();

                let k = cxt.fresh_block();
                cxt.push_loop(k);
                for i in block {
                    i.lower(cxt);
                }
                let block = cxt.pop_block();

                cxt.block.push(JStmt::While(k, cond, block));
            }
            Statement::InlineJava(s) => {
                cxt.block.push(JStmt::InlineJava(*s));
            }
        }
    }
}
impl Item {
    fn lower(&self, cxt: &mut Cxt) {
        match self {
            // Module-level inline java is handled by codegen()
            Item::InlineJava(_) => (),
            Item::Fn(f) => {
                let mut block = Vec::new();
                let fn_id = cxt.fun(f.id).unwrap();
                std::mem::swap(&mut block, &mut cxt.block);

                cxt.push();
                cxt.current_fn = fn_id;
                let mut args = Vec::new();
                for (name, ty) in &f.args {
                    let mut vars = Vec::new();
                    for ty in ty.lower(cxt) {
                        let var = cxt.fresh_var(cxt.bindings.public(*name));
                        args.push((name.raw(), var, ty.clone()));
                        cxt.tys.insert(var, ty);
                        vars.push(var);
                    }
                    cxt.vars.push((name.clone(), JVars::Tuple(vars)));
                }
                let ret = f.body.lower(cxt);
                match (ret, &f.ret_ty) {
                    // Java doesn't like using 'return' with void functions
                    (ret, Type::Unit) => {
                        for i in ret {
                            cxt.block.push(JStmt::Term(i))
                        }
                    }
                    (ret, _) => cxt.block.push(JStmt::Ret(fn_id, ret.into())),
                }
                cxt.pop();

                std::mem::swap(&mut block, &mut cxt.block);
                let ret_ty = f.ret_ty.lower(cxt);
                cxt.items.push(JItem::Fn(JFn {
                    name: cxt.bindings.fn_name(f.id),
                    fn_id,
                    ret_tys: ret_ty.into(),
                    args,
                    body: block,
                    public: f.public,
                }));
            }
            Item::Enum(tid, variants, ext) => {
                if !ext {
                    let class = cxt.class(*tid).unwrap();

                    cxt.items.push(JItem::Enum(class, variants.clone()));
                }
            }
            Item::ExternFn(_) => (),
            Item::ExternClass(_) => (),
        }
    }
}
impl Type {
    fn lower(&self, cxt: &Cxt) -> JTys {
        JTys::One(match self {
            Type::I32 => JTy::I32,
            Type::I64 => JTy::I64,
            Type::Bool => JTy::Bool,
            Type::Str => JTy::String,
            Type::Unit => return JTys::empty(),
            Type::Class(c) => JTy::Class(cxt.class(*c).unwrap()),
            Type::Tuple(v) => return JTys::Tuple(v.iter().flat_map(|x| x.lower(cxt)).collect()),
        })
    }
}
