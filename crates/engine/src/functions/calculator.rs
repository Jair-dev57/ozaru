use super::{clip, invalid};
use crate::base::Result;

/// Type 4, a program of the PostScript calculator (ISO 32000-2, section
/// 7.10.5): the inputs are pushed on a stack, the program runs, and the
/// outputs are what is left on it.
///
/// It has numbers, booleans, the arithmetic, relational, boolean and stack
/// operators of section 7.10.5.2, and `if` and `ifelse`, but no loops, so
/// every program ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Calculator {
    domain: Vec<f64>,
    range: Vec<f64>,
    program: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
enum Item {
    Number(Value),
    Operator(Operator),
    If(Vec<Item>),
    IfElse(Vec<Item>, Vec<Item>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Integer(i64),
    Real(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Abs,
    Add,
    Atan,
    Ceiling,
    Cos,
    Cvi,
    Cvr,
    Div,
    Exp,
    Floor,
    Idiv,
    Ln,
    Log,
    Mod,
    Mul,
    Neg,
    Round,
    Sin,
    Sqrt,
    Sub,
    Truncate,
    And,
    Bitshift,
    Eq,
    False,
    Ge,
    Gt,
    Le,
    Lt,
    Ne,
    Not,
    Or,
    True,
    Xor,
    Copy,
    Dup,
    Exch,
    Index,
    Pop,
    Roll,
}

// The deepest the stack can be, as the standard says.
const MAX_STACK: usize = 100;
// How deep blocks can be in each other.
const MAX_NESTING: usize = 100;

impl Calculator {
    pub(super) fn new(program: &[u8], domain: Vec<f64>, range: Vec<f64>) -> Result<Self> {
        let mut tokens = Tokens {
            input: program,
            at: 0,
        };
        // The program is a block: `{ ... }`.
        if tokens.next() != Some(Token::Open) {
            return Err(invalid());
        }
        let program = block(&mut tokens, 0)?;
        Ok(Self {
            domain,
            range,
            program,
        })
    }

    pub(super) fn inputs(&self) -> usize {
        self.domain.len() / 2
    }

    pub(super) fn outputs(&self) -> usize {
        self.range.len() / 2
    }

    pub(super) fn evaluate(&self, input: &mut [f64]) -> Vec<f64> {
        let mut stack: Vec<Value> = Vec::with_capacity(MAX_STACK);
        for (i, x) in input.iter().enumerate() {
            let (d0, d1) = (self.domain[2 * i], self.domain[2 * i + 1]);
            stack.push(Value::Real(x.clamp(d0.min(d1), d1.max(d0))));
        }
        let outputs = self.outputs();
        let ran = run(&self.program, &mut stack).is_some() && stack.len() >= outputs;
        let mut out: Vec<f64> = if ran {
            stack[stack.len() - outputs..]
                .iter()
                .map(|value| value.number().filter(|n| n.is_finite()).unwrap_or(0.0))
                .collect()
        } else {
            // A program that fails gives the lowest values of the range.
            self.range
                .chunks(2)
                .map(|pair| pair[0].min(pair[1]))
                .collect()
        };
        clip(&mut out, Some(&self.range));
        out
    }
}

// The items of a block whose `{` was read, up to its `}`.
fn block(tokens: &mut Tokens<'_>, nesting: usize) -> Result<Vec<Item>> {
    if nesting > MAX_NESTING {
        return Err(invalid());
    }
    let mut items = Vec::new();
    // Blocks read and not yet taken by `if` or `ifelse`.
    let mut blocks: Vec<Vec<Item>> = Vec::new();
    loop {
        match tokens.next().ok_or_else(invalid)? {
            Token::Close => break,
            Token::Open => blocks.push(block(tokens, nesting + 1)?),
            Token::Number(value) => items.push(Item::Number(value)),
            Token::Word(b"if") => {
                let then = blocks.pop().ok_or_else(invalid)?;
                items.push(Item::If(then));
            }
            Token::Word(b"ifelse") => {
                let otherwise = blocks.pop().ok_or_else(invalid)?;
                let then = blocks.pop().ok_or_else(invalid)?;
                items.push(Item::IfElse(then, otherwise));
            }
            Token::Word(word) => items.push(Item::Operator(operator(word).ok_or_else(invalid)?)),
        }
        // A block is only valid right before `if` or `ifelse`.
        if blocks.len() > 2 {
            return Err(invalid());
        }
    }
    if !blocks.is_empty() {
        return Err(invalid());
    }
    Ok(items)
}

fn operator(word: &[u8]) -> Option<Operator> {
    use Operator::*;
    Some(match word {
        b"abs" => Abs,
        b"add" => Add,
        b"atan" => Atan,
        b"ceiling" => Ceiling,
        b"cos" => Cos,
        b"cvi" => Cvi,
        b"cvr" => Cvr,
        b"div" => Div,
        b"exp" => Exp,
        b"floor" => Floor,
        b"idiv" => Idiv,
        b"ln" => Ln,
        b"log" => Log,
        b"mod" => Mod,
        b"mul" => Mul,
        b"neg" => Neg,
        b"round" => Round,
        b"sin" => Sin,
        b"sqrt" => Sqrt,
        b"sub" => Sub,
        b"truncate" => Truncate,
        b"and" => And,
        b"bitshift" => Bitshift,
        b"eq" => Eq,
        b"false" => False,
        b"ge" => Ge,
        b"gt" => Gt,
        b"le" => Le,
        b"lt" => Lt,
        b"ne" => Ne,
        b"not" => Not,
        b"or" => Or,
        b"true" => True,
        b"xor" => Xor,
        b"copy" => Copy,
        b"dup" => Dup,
        b"exch" => Exch,
        b"index" => Index,
        b"pop" => Pop,
        b"roll" => Roll,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token<'a> {
    Open,
    Close,
    Number(Value),
    Word(&'a [u8]),
}

struct Tokens<'a> {
    input: &'a [u8],
    at: usize,
}

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Option<Token<'a>> {
        loop {
            let byte = *self.input.get(self.at)?;
            match byte {
                b'%' => {
                    while self
                        .input
                        .get(self.at)
                        .is_some_and(|&b| b != b'\n' && b != b'\r')
                    {
                        self.at += 1;
                    }
                }
                b'{' => {
                    self.at += 1;
                    return Some(Token::Open);
                }
                b'}' => {
                    self.at += 1;
                    return Some(Token::Close);
                }
                b if b.is_ascii_whitespace() || b == 0 => self.at += 1,
                _ => break,
            }
        }
        let start = self.at;
        while self
            .input
            .get(self.at)
            .is_some_and(|&b| !b.is_ascii_whitespace() && !matches!(b, b'{' | b'}' | b'%' | 0))
        {
            self.at += 1;
        }
        let word = &self.input[start..self.at];
        let text = std::str::from_utf8(word).ok();
        let number = text.and_then(|text| {
            let looks_numeric = text
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'));
            if !looks_numeric || !text.bytes().any(|b| b.is_ascii_digit()) {
                return None;
            }
            match text.parse::<i64>() {
                Ok(integer) => Some(Value::Integer(integer)),
                Err(_) => text
                    .parse::<f64>()
                    .ok()
                    .filter(|r| r.is_finite())
                    .map(Value::Real),
            }
        });
        Some(match number {
            Some(value) => Token::Number(value),
            None => Token::Word(word),
        })
    }
}

impl Value {
    fn number(self) -> Option<f64> {
        match self {
            Self::Integer(i) => Some(i as f64),
            Self::Real(r) => Some(r),
            Self::Boolean(_) => None,
        }
    }
}

// Runs `items` on `stack`. `None` on an error: a stack too deep or too
// shallow, or an operand of the wrong kind.
fn run(items: &[Item], stack: &mut Vec<Value>) -> Option<()> {
    for item in items {
        match item {
            Item::Number(value) => push(stack, *value)?,
            Item::If(then) => {
                if pop_bool(stack)? {
                    run(then, stack)?;
                }
            }
            Item::IfElse(then, otherwise) => {
                if pop_bool(stack)? {
                    run(then, stack)?;
                } else {
                    run(otherwise, stack)?;
                }
            }
            Item::Operator(operator) => apply(*operator, stack)?,
        }
    }
    Some(())
}

fn push(stack: &mut Vec<Value>, value: Value) -> Option<()> {
    if stack.len() >= MAX_STACK {
        return None;
    }
    stack.push(value);
    Some(())
}

fn pop(stack: &mut Vec<Value>) -> Option<Value> {
    stack.pop()
}

fn pop_number(stack: &mut Vec<Value>) -> Option<f64> {
    pop(stack)?.number()
}

fn pop_integer(stack: &mut Vec<Value>) -> Option<i64> {
    match pop(stack)? {
        Value::Integer(i) => Some(i),
        _ => None,
    }
}

fn pop_bool(stack: &mut Vec<Value>) -> Option<bool> {
    match pop(stack)? {
        Value::Boolean(b) => Some(b),
        _ => None,
    }
}

// An integer when the result is one and fits, as PostScript keeps integers.
fn integer_or_real(value: Option<i64>, real: f64) -> Value {
    match value {
        Some(i) => Value::Integer(i),
        None => Value::Real(real),
    }
}

fn apply(operator: Operator, stack: &mut Vec<Value>) -> Option<()> {
    use Operator::*;
    let value = match operator {
        Add | Sub | Mul => {
            let b = pop(stack)?;
            let a = pop(stack)?;
            match (a, b) {
                (Value::Integer(a), Value::Integer(b)) => {
                    let exact = match operator {
                        Add => a.checked_add(b),
                        Sub => a.checked_sub(b),
                        _ => a.checked_mul(b),
                    };
                    let (a, b) = (a as f64, b as f64);
                    let real = match operator {
                        Add => a + b,
                        Sub => a - b,
                        _ => a * b,
                    };
                    integer_or_real(exact, real)
                }
                (a, b) => {
                    let (a, b) = (a.number()?, b.number()?);
                    Value::Real(match operator {
                        Add => a + b,
                        Sub => a - b,
                        _ => a * b,
                    })
                }
            }
        }
        Div => {
            let b = pop_number(stack)?;
            let a = pop_number(stack)?;
            if b == 0.0 {
                return None;
            }
            Value::Real(a / b)
        }
        Idiv | Mod => {
            let b = pop_integer(stack)?;
            let a = pop_integer(stack)?;
            let result = if operator == Idiv {
                a.checked_div(b)
            } else {
                a.checked_rem(b)
            };
            Value::Integer(result?)
        }
        Neg | Abs => match pop(stack)? {
            Value::Integer(i) => {
                let exact = if operator == Neg {
                    i.checked_neg()
                } else {
                    i.checked_abs()
                };
                let real = if operator == Neg {
                    -(i as f64)
                } else {
                    (i as f64).abs()
                };
                integer_or_real(exact, real)
            }
            Value::Real(r) => Value::Real(if operator == Neg { -r } else { r.abs() }),
            Value::Boolean(_) => return None,
        },
        Ceiling | Floor | Round | Truncate => match pop(stack)? {
            Value::Integer(i) => Value::Integer(i),
            Value::Real(r) => Value::Real(match operator {
                Ceiling => r.ceil(),
                Floor => r.floor(),
                // PostScript rounds halves up.
                Round => (r + 0.5).floor(),
                _ => r.trunc(),
            }),
            Value::Boolean(_) => return None,
        },
        Sqrt | Sin | Cos | Ln | Log => {
            let x = pop_number(stack)?;
            Value::Real(match operator {
                Sqrt => x.sqrt(),
                // Angles are in degrees.
                Sin => x.to_radians().sin(),
                Cos => x.to_radians().cos(),
                Ln => x.ln(),
                _ => x.log10(),
            })
        }
        Atan => {
            let den = pop_number(stack)?;
            let num = pop_number(stack)?;
            if num == 0.0 && den == 0.0 {
                return None;
            }
            Value::Real(num.atan2(den).to_degrees().rem_euclid(360.0))
        }
        Exp => {
            let exponent = pop_number(stack)?;
            let base = pop_number(stack)?;
            Value::Real(base.powf(exponent))
        }
        Cvi => {
            let x = pop_number(stack)?.trunc();
            if !(-9.2e18..9.2e18).contains(&x) {
                return None;
            }
            Value::Integer(x as i64)
        }
        Cvr => Value::Real(pop_number(stack)?),
        And | Or | Xor => match (pop(stack)?, pop(stack)?) {
            (Value::Boolean(b), Value::Boolean(a)) => Value::Boolean(match operator {
                And => a && b,
                Or => a || b,
                _ => a ^ b,
            }),
            (Value::Integer(b), Value::Integer(a)) => Value::Integer(match operator {
                And => a & b,
                Or => a | b,
                _ => a ^ b,
            }),
            _ => return None,
        },
        Not => match pop(stack)? {
            Value::Boolean(b) => Value::Boolean(!b),
            Value::Integer(i) => Value::Integer(!i),
            Value::Real(_) => return None,
        },
        Bitshift => {
            let shift = pop_integer(stack)?;
            let value = pop_integer(stack)?;
            Value::Integer(match shift {
                0..=63 => value << shift,
                -63..=-1 => value >> -shift,
                _ => 0,
            })
        }
        True => Value::Boolean(true),
        False => Value::Boolean(false),
        Eq | Ne => {
            let b = pop(stack)?;
            let a = pop(stack)?;
            let equal = match (a, b) {
                (Value::Boolean(a), Value::Boolean(b)) => a == b,
                (a, b) => a.number()? == b.number()?,
            };
            Value::Boolean(if operator == Eq { equal } else { !equal })
        }
        Ge | Gt | Le | Lt => {
            let b = pop_number(stack)?;
            let a = pop_number(stack)?;
            Value::Boolean(match operator {
                Ge => a >= b,
                Gt => a > b,
                Le => a <= b,
                _ => a < b,
            })
        }
        Dup => {
            let top = *stack.last()?;
            return push(stack, top);
        }
        Pop => {
            pop(stack)?;
            return Some(());
        }
        Exch => {
            let b = pop(stack)?;
            let a = pop(stack)?;
            stack.push(b);
            stack.push(a);
            return Some(());
        }
        Copy => {
            let n = usize::try_from(pop_integer(stack)?).ok()?;
            if n > stack.len() || stack.len() + n > MAX_STACK {
                return None;
            }
            let start = stack.len() - n;
            stack.extend_from_within(start..);
            return Some(());
        }
        Index => {
            let n = usize::try_from(pop_integer(stack)?).ok()?;
            let value = *stack.get(stack.len().checked_sub(n + 1)?)?;
            return push(stack, value);
        }
        Roll => {
            let j = pop_integer(stack)?;
            let n = usize::try_from(pop_integer(stack)?).ok()?;
            if n > stack.len() {
                return None;
            }
            if n > 0 {
                let start = stack.len() - n;
                let shift = j.rem_euclid(n as i64) as usize;
                stack[start..].rotate_right(shift);
            }
            return Some(());
        }
    };
    push(stack, value)
}
