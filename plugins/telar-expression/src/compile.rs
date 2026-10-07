//! The type checker, and [`Compiled`]: a parsed expression whose every reference, call and operator has been checked, ready to evaluate as often as its inputs move.

use std::fmt;
use std::sync::Arc;

use crate::registry::{Function, Implementation};
use crate::syntax::{BinaryOp, Expr, ExprKind, UnaryOp, parse};
use crate::types::Mismatch;
use crate::{
    Context, Environment, Error, ErrorCode, Errors, Pattern, Reference, Registry, Resolver, Span,
    Type, Value, eval,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

pub(crate) enum NodeKind {
    Literal(Value),
    List(Vec<Node>),
    Reference(Reference, Type),
    Negate(Box<Node>),
    Not(Box<Node>),
    Hundredth(Box<Node>),
    Arithmetic(Arithmetic, Box<Node>, Box<Node>),
    /// `base ± percent`, where the percentage is taken of `base`: `200 + 10%` is 220.
    Relative {
        subtract: bool,
        base: Box<Node>,
        percent: Box<Node>,
    },
    Concat(Box<Node>, Box<Node>),
    Compare(Comparison, Box<Node>, Box<Node>),
    Equal {
        negate: bool,
        left: Box<Node>,
        right: Box<Node>,
    },
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    If {
        condition: Box<Node>,
        then: Box<Node>,
        otherwise: Box<Node>,
    },
    Call {
        name: String,
        implementation: Arc<Implementation>,
        returns: Type,
        arguments: Vec<Node>,
    },
    /// Stands in for a part that failed to check. Never evaluated, since an expression with any error does not compile.
    Invalid,
}

pub(crate) struct Node {
    pub kind: NodeKind,
    pub span: Span,
}

impl Node {
    fn new(kind: NodeKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// A checked expression, ready to evaluate.
///
/// Cheap to clone and `Send + Sync`: it can be compiled where the configuration is loaded and evaluated wherever the values live.
#[derive(Clone)]
pub struct Compiled(Arc<Inner>);

struct Inner {
    source: String,
    root: Node,
    ty: Type,
    references: Vec<Reference>,
}

impl Compiled {
    pub fn source(&self) -> &str {
        &self.0.source
    }

    /// The type every successful evaluation produces.
    pub fn ty(&self) -> &Type {
        &self.0.ty
    }

    /// Every reference the expression can read, once each, in the order written. What it reads on a given evaluation may be fewer: an `if` reads one branch.
    pub fn references(&self) -> &[Reference] {
        &self.0.references
    }

    /// Evaluates the expression against the current values `resolver` reads. Never panics: everything that can go wrong is an [`Error`] of kind [`crate::ErrorKind::Evaluation`].
    pub fn evaluate(&self, resolver: &dyn Resolver) -> Result<Value, Error> {
        eval::evaluate(&self.0.root, resolver)
    }

    /// This expression, if its type can stand where `expected` is wanted — a property that takes a colour, a condition that must be a bool. The error covers the whole expression.
    pub fn require(self, expected: &Type) -> Result<Self, Error> {
        if expected.accepts(self.ty()) {
            return Ok(self);
        }
        Err(Error::mismatch(
            Span::new(0, self.0.source.len()),
            ErrorCode::ExpectedType {
                expected: expected.clone(),
                found: self.ty().clone(),
            },
        ))
    }
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Compiled")
            .field("source", &self.0.source)
            .field("ty", &self.0.ty)
            .finish_non_exhaustive()
    }
}

/// Parses and type-checks `source`: references against `environment`, calls and constants against `registry`.
///
/// A syntax error stops at the first one, since what follows it cannot be read reliably. Type errors are all reported, each at its own span, and a part that failed is not blamed again by what contains it.
pub fn compile(
    source: &str,
    environment: &dyn Environment,
    registry: &Registry,
) -> Result<Compiled, Errors> {
    let expr = parse(source)?;
    let mut checker = Checker {
        environment,
        registry,
        errors: Vec::new(),
        references: Vec::new(),
    };
    let (root, ty) = checker.check(&expr);
    if let Some(errors) = Errors::from_vec(checker.errors) {
        return Err(errors);
    }
    Ok(Compiled(Arc::new(Inner {
        source: source.to_string(),
        root,
        ty,
        references: checker.references,
    })))
}

struct Checker<'a> {
    environment: &'a dyn Environment,
    registry: &'a Registry,
    errors: Vec<Error>,
    references: Vec<Reference>,
}

impl Checker<'_> {
    fn fail(&mut self, span: Span, code: ErrorCode) -> (Node, Type) {
        self.errors.push(Error::mismatch(span, code));
        (Node::new(NodeKind::Invalid, span), Type::Never)
    }

    fn require(&mut self, ty: &Type, expected: &Type, span: Span, context: Context) {
        if !expected.accepts(ty) {
            self.errors.push(Error::mismatch(
                span,
                ErrorCode::OperandType {
                    context,
                    expected: expected.clone(),
                    found: ty.clone(),
                },
            ));
        }
    }

    fn operand(&mut self, expr: &Expr, expected: &Type, context: Context) -> Box<Node> {
        let (node, ty) = self.check(expr);
        self.require(&ty, expected, expr.span, context);
        Box::new(node)
    }

    fn check(&mut self, expr: &Expr) -> (Node, Type) {
        let span = expr.span;
        let literal = |value: Value| {
            let ty = value.type_of();
            (Node::new(NodeKind::Literal(value), span), ty)
        };
        match &expr.kind {
            ExprKind::Number(n) => literal(Value::Number(*n)),
            ExprKind::Text(text) => literal(Value::Text(Arc::clone(text))),
            ExprKind::Bool(b) => literal(Value::Bool(*b)),
            ExprKind::Color(color) => literal(Value::Color(*color)),
            ExprKind::List(items) => self.list(items, span),
            ExprKind::Reference(reference) => self.reference(reference, span),
            ExprKind::Name(name) => self.name(name, span),
            ExprKind::Percent(inner) => {
                let inner = self.operand(inner, &Type::Number, Context::Operator("%"));
                (Node::new(NodeKind::Hundredth(inner), span), Type::Number)
            }
            ExprKind::Unary(op, inner) => self.unary(*op, inner, span),
            ExprKind::Binary {
                op,
                op_span,
                left,
                right,
            } => self.binary(*op, *op_span, left, right, span),
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.operand(condition, &Type::Bool, Context::IfCondition);
                let (then_node, then_ty) = self.check(then);
                let (otherwise_node, otherwise_ty) = self.check(otherwise);
                let Some(ty) = then_ty.unify(&otherwise_ty) else {
                    return self.fail(
                        otherwise.span,
                        ErrorCode::BranchMismatch {
                            first: then_ty,
                            second: otherwise_ty,
                        },
                    );
                };
                let node = NodeKind::If {
                    condition,
                    then: Box::new(then_node),
                    otherwise: Box::new(otherwise_node),
                };
                (Node::new(node, span), ty)
            }
            ExprKind::Call {
                name,
                name_span,
                arguments,
            } => self.call(name, *name_span, arguments, span),
        }
    }

    fn list(&mut self, items: &[Expr], span: Span) -> (Node, Type) {
        let mut element = Type::Never;
        let mut nodes = Vec::with_capacity(items.len());
        for item in items {
            let (node, ty) = self.check(item);
            match element.unify(&ty) {
                Some(unified) => element = unified,
                None => {
                    self.errors.push(Error::mismatch(
                        item.span,
                        ErrorCode::ListItemMismatch {
                            item: ty,
                            list: element.clone(),
                        },
                    ));
                }
            }
            nodes.push(node);
        }
        (Node::new(NodeKind::List(nodes), span), Type::list(element))
    }

    fn reference(&mut self, reference: &Reference, span: Span) -> (Node, Type) {
        match self.environment.reference(reference) {
            Ok(ty) => {
                if !self.references.contains(reference) {
                    self.references.push(reference.clone());
                }
                let node = NodeKind::Reference(reference.clone(), ty.clone());
                (Node::new(node, span), ty)
            }
            Err(error) => self.fail(span, ErrorCode::Host(error)),
        }
    }

    fn name(&mut self, name: &str, span: Span) -> (Node, Type) {
        if let Some(constant) = self.registry.lookup_constant(name) {
            let value = constant.value.clone();
            let ty = value.type_of();
            return (Node::new(NodeKind::Literal(value), span), ty);
        }
        if !self.registry.forms(name).is_empty() {
            return self.fail(
                span,
                ErrorCode::NameIsFunction {
                    name: name.to_string(),
                },
            );
        }
        self.fail(
            span,
            ErrorCode::UnknownName {
                name: name.to_string(),
            },
        )
    }

    fn unary(&mut self, op: UnaryOp, inner: &Expr, span: Span) -> (Node, Type) {
        match op {
            UnaryOp::Negate => {
                let inner = self.operand(inner, &Type::Number, Context::Operator("-"));
                (Node::new(NodeKind::Negate(inner), span), Type::Number)
            }
            UnaryOp::Plus => {
                let mut inner = self.operand(inner, &Type::Number, Context::Operator("+"));
                inner.span = span;
                (*inner, Type::Number)
            }
            UnaryOp::Not => {
                let inner = self.operand(inner, &Type::Bool, Context::Operator("!"));
                (Node::new(NodeKind::Not(inner), span), Type::Bool)
            }
        }
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        op_span: Span,
        left: &Expr,
        right: &Expr,
        span: Span,
    ) -> (Node, Type) {
        let context = Context::Operator(op.symbol());
        match op {
            BinaryOp::Add => self.add(left, right, span),
            BinaryOp::Subtract if right.is_percent() => {
                let base = self.operand(left, &Type::Number, context);
                let percent = self.operand(right, &Type::Number, context);
                let node = NodeKind::Relative {
                    subtract: true,
                    base,
                    percent,
                };
                (Node::new(node, span), Type::Number)
            }
            BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Power => {
                let left = self.operand(left, &Type::Number, context);
                let right = self.operand(right, &Type::Number, context);
                let arithmetic = match op {
                    BinaryOp::Subtract => Arithmetic::Subtract,
                    BinaryOp::Multiply => Arithmetic::Multiply,
                    BinaryOp::Divide => Arithmetic::Divide,
                    _ => Arithmetic::Power,
                };
                let node = NodeKind::Arithmetic(arithmetic, left, right);
                (Node::new(node, span), Type::Number)
            }
            BinaryOp::And | BinaryOp::Or => {
                let left = self.operand(left, &Type::Bool, context);
                let right = self.operand(right, &Type::Bool, context);
                let node = if op == BinaryOp::And {
                    NodeKind::And(left, right)
                } else {
                    NodeKind::Or(left, right)
                };
                (Node::new(node, span), Type::Bool)
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                let (left_node, left_ty) = self.check(left);
                let (right_node, right_ty) = self.check(right);
                if left_ty.unify(&right_ty).is_none() {
                    return self.fail(
                        right.span,
                        ErrorCode::CompareMismatch {
                            operator: op.symbol(),
                            left: left_ty,
                            right: right_ty,
                        },
                    );
                }
                let node = NodeKind::Equal {
                    negate: op == BinaryOp::NotEqual,
                    left: Box::new(left_node),
                    right: Box::new(right_node),
                };
                (Node::new(node, span), Type::Bool)
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                let (left_node, left_ty) = self.check(left);
                let (right_node, right_ty) = self.check(right);
                let Some(ty) = left_ty.unify(&right_ty) else {
                    return self.fail(
                        right.span,
                        ErrorCode::CompareMismatch {
                            operator: op.symbol(),
                            left: left_ty,
                            right: right_ty,
                        },
                    );
                };
                if !matches!(ty, Type::Number | Type::Text | Type::Never) {
                    return self.fail(
                        op_span,
                        ErrorCode::NotOrderable {
                            operator: op.symbol(),
                            found: ty,
                        },
                    );
                }
                let comparison = match op {
                    BinaryOp::Less => Comparison::Less,
                    BinaryOp::LessEqual => Comparison::LessEqual,
                    BinaryOp::Greater => Comparison::Greater,
                    _ => Comparison::GreaterEqual,
                };
                let node = NodeKind::Compare(comparison, Box::new(left_node), Box::new(right_node));
                (Node::new(node, span), Type::Bool)
            }
        }
    }

    fn add(&mut self, left: &Expr, right: &Expr, span: Span) -> (Node, Type) {
        let (left_node, left_ty) = self.check(left);
        let (right_node, right_ty) = self.check(right);
        let joins_text =
            left_ty == Type::Text || (left_ty == Type::Never && right_ty == Type::Text);
        if joins_text {
            self.require(&right_ty, &Type::Text, right.span, Context::AddAfterText);
            let node = NodeKind::Concat(Box::new(left_node), Box::new(right_node));
            return (Node::new(node, span), Type::Text);
        }
        self.require(&left_ty, &Type::Number, left.span, Context::Operator("+"));
        self.require(&right_ty, &Type::Number, right.span, Context::Operator("+"));
        let node = if right.is_percent() {
            NodeKind::Relative {
                subtract: false,
                base: Box::new(left_node),
                percent: Box::new(right_node),
            }
        } else {
            NodeKind::Arithmetic(Arithmetic::Add, Box::new(left_node), Box::new(right_node))
        };
        (Node::new(node, span), Type::Number)
    }

    fn call(
        &mut self,
        name: &str,
        name_span: Span,
        arguments: &[Expr],
        span: Span,
    ) -> (Node, Type) {
        let mut nodes = Vec::with_capacity(arguments.len());
        let mut types = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let (node, ty) = self.check(argument);
            nodes.push(node);
            types.push(ty);
        }
        let forms = self.registry.forms(name);
        if forms.is_empty() {
            if self.registry.lookup_constant(name).is_some() {
                return self.fail(
                    name_span,
                    ErrorCode::NameIsConstant {
                        name: name.to_string(),
                    },
                );
            }
            return self.fail(
                name_span,
                ErrorCode::UnknownFunction {
                    name: name.to_string(),
                },
            );
        }
        for form in forms.iter().rev() {
            if let Ok(returns) = form.signature().apply(&types) {
                let node = NodeKind::Call {
                    name: form.name().to_string(),
                    implementation: Arc::clone(form.implementation()),
                    returns: returns.clone(),
                    arguments: nodes,
                };
                return (Node::new(node, span), returns);
            }
        }
        let mut same_count = forms
            .iter()
            .filter(|form| form.signature().apply(&types) != Err(Mismatch::Count));
        if let (Some(form), None) = (same_count.next(), same_count.next()) {
            return self.mismatch(form, arguments, &types, span);
        }
        if let [form] = forms {
            return self.mismatch(form, arguments, &types, span);
        }
        self.fail(
            span,
            ErrorCode::NoMatchingForm {
                name: forms[0].name().to_string(),
                given: types,
                forms: forms.iter().map(|form| form.signature().clone()).collect(),
            },
        )
    }

    fn mismatch(
        &mut self,
        form: &Function,
        arguments: &[Expr],
        types: &[Type],
        span: Span,
    ) -> (Node, Type) {
        let signature = form.signature();
        let name = form.name();
        match signature.apply(types) {
            Err(Mismatch::Argument(index)) if index < arguments.len() => {
                let expected = signature.parameter(index).cloned().unwrap_or(Pattern::Any);
                self.fail(
                    arguments[index].span,
                    ErrorCode::ArgumentType {
                        name: name.to_string(),
                        index,
                        expected,
                        found: types[index].clone(),
                    },
                )
            }
            _ => {
                let (takes, at_least) = signature.arity();
                self.fail(
                    span,
                    ErrorCode::ArgumentCount {
                        name: name.to_string(),
                        takes,
                        at_least,
                        given: types.len(),
                    },
                )
            }
        }
    }
}

#[cfg(test)]
#[path = "compile_test.rs"]
mod tests;
