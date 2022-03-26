use std::collections::HashMap;
use std::iter::Peekable;
use std::io::{stdin, stdout};
use std::io::Write;
use std::fmt;

mod lexer;

use lexer::*;

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Sym(String),
    Fun(String, Vec<Expr>)
}

#[derive(Debug)]
enum Error {
    UnexpectedToken(TokenKindSet, Token),
    RuleAlreadyExists(String, Loc, Loc),
    RuleDoesNotExist(String),
    AlreadyShaping,
    NoShapingInPlace,
}

impl Expr {
    fn parse_peekable(lexer: &mut Peekable<impl Iterator<Item=Token>>) -> Result<Self, Error> {
        use TokenKind::*;
        let name = lexer.next().expect("Completely exhausted lexer");
        match name.kind {
            Sym => {
                if let Some(_) = lexer.next_if(|t| t.kind == OpenParen) {
                    let mut args = Vec::new();
                    if let Some(_) = lexer.next_if(|t| t.kind == CloseParen) {
                        return Ok(Expr::Fun(name.text, args))
                    }
                    args.push(Self::parse_peekable(lexer)?);
                    while let Some(_) = lexer.next_if(|t| t.kind == Comma) {
                        args.push(Self::parse_peekable(lexer)?);
                    }
                    let close_paren = lexer.next().expect("Completely exhausted lexer");
                    if close_paren.kind == CloseParen {
                        Ok(Expr::Fun(name.text, args))
                    } else {
                        Err(Error::UnexpectedToken(TokenKindSet::single(CloseParen), close_paren))
                    }
                } else {
                    Ok(Expr::Sym(name.text))
                }
            },
            _ => Err(Error::UnexpectedToken(TokenKindSet::single(Sym), name))
        }
    }

    fn parse(lexer: &mut impl Iterator<Item=Token>) -> Result<Self, Error> {
        Self::parse_peekable(&mut lexer.peekable())
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Sym(name) => write!(f, "{}", name),
            Expr::Fun(name, args) => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")? }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            },
        }
    }
}

#[derive(Debug)]
struct Rule {
    loc: Loc,
    head: Expr,
    body: Expr,
}

fn substitute_bindings(bindings: &Bindings, expr: &Expr) -> Expr {
    use Expr::*;
    match expr {
        Sym(name) => {
            if let Some(value) = bindings.get(name) {
                value.clone()
            } else {
                expr.clone()
            }
        },

        Fun(name, args) => {
            let new_name = match bindings.get(name) {
                Some(Sym(new_name)) => new_name.clone(),
                None => name.clone(),
                Some(_) => todo!("Report expected symbol in the place of the functor name"),
            };
            let mut new_args = Vec::new();
            for arg in args {
                new_args.push(substitute_bindings(bindings, &arg))
            }
            Fun(new_name, new_args)
        }
    }
}

fn expect_token_kind(lexer: &mut Peekable<impl Iterator<Item=Token>>, kinds: TokenKindSet) -> Result<Token, Error> {
    let token = lexer.next().expect("Completely exhausted lexer");
    if kinds.contains(token.kind) {
        Ok(token)
    } else {
        Err(Error::UnexpectedToken(kinds, token))
    }
}

impl Rule {
    fn apply_all(&self, expr: &Expr) -> Expr {
        if let Some(bindings) = pattern_match(&self.head, expr) {
            substitute_bindings(&bindings, &self.body)
        } else {
            use Expr::*;
            match expr {
                Sym(_) => expr.clone(),
                Fun(name, args) => {
                    let mut new_args = Vec::new();
                    for arg in args {
                        new_args.push(self.apply_all(arg))
                    }
                    Fun(name.clone(), new_args)
                }
            }
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} = {}", self.head, self.body)
    }
}

type Bindings = HashMap<String, Expr>;

fn pattern_match(pattern: &Expr, value: &Expr) -> Option<Bindings> {
    fn pattern_match_impl(pattern: &Expr, value: &Expr, bindings: &mut Bindings) -> bool {
        use Expr::*;
        match (pattern, value) {
            (Sym(name), _) => {
                if let Some(bound_value) = bindings.get(name) {
                    bound_value == value
                } else {
                    bindings.insert(name.clone(), value.clone());
                    true
                }
            },
            (Fun(name1, args1), Fun(name2, args2)) => {
                if name1 == name2 && args1.len() == args2.len() {
                    for i in 0..args1.len() {
                        if !pattern_match_impl(&args1[i], &args2[i], bindings) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            },
            _ => false,
        }
    }

    let mut bindings = HashMap::new();

    if pattern_match_impl(pattern, value, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

#[allow(unused_macros)]
macro_rules! fun_args {
    () => { vec![] };
    ($name:ident) => { vec![expr!($name)] };
    ($name:ident,$($rest:tt)*) => {
        {
            let mut t = vec![expr!($name)];
            t.append(&mut fun_args!($($rest)*));
            t
        }
    };
    ($name:ident($($args:tt)*)) => {
        vec![expr!($name($($args)*))]
    };
    ($name:ident($($args:tt)*),$($rest:tt)*) => {
        {
            let mut t = vec![expr!($name($($args)*))];
            t.append(&mut fun_args!($($rest)*));
            t
        }
    }
}

#[allow(unused_macros)]
macro_rules! expr {
    ($name:ident) => {
        Expr::Sym(stringify!($name).to_string())
    };
    ($name:ident($($args:tt)*)) => {
        Expr::Fun(stringify!($name).to_string(), fun_args!($($args)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn rule_apply_all() {
        // swap(pair(a, b)) = pair(b, a)
        let swap = Rule {
            head: expr!(swap(pair(a, b))),
            body: expr!(pair(b, a)),
        };

        let input = expr! {
            foo(swap(pair(f(a), g(b))),
                swap(pair(q(c), z(d))))
        };

        let expected = expr! {
            foo(pair(g(b), f(a)),
                pair(z(d), q(c)))
        };

        assert_eq!(swap.apply_all(&input), expected);
    }
}

#[derive(Default)]
struct Context {
    rules: HashMap<String, Rule>,
    current_expr: Option<Expr>
}

impl Context {
    fn process_command(&mut self, lexer: &mut Peekable<impl Iterator<Item=Token>>) -> Result<(), Error> {
        let expected_tokens = TokenKindSet::empty()
            .set(TokenKind::Rule)
            .set(TokenKind::Shape)
            .set(TokenKind::Apply)
            .set(TokenKind::Done);
        let keyword = expect_token_kind(lexer, expected_tokens)?;
        match keyword.kind {
            TokenKind::Rule => {
                let name = expect_token_kind(lexer, TokenKindSet::single(TokenKind::Sym))?;
                if let Some(existing_rule) = self.rules.get(&name.text) {
                    return Err(Error::RuleAlreadyExists(name.text, name.loc, existing_rule.loc.clone()))
                }
                let head = Expr::parse_peekable(lexer)?;
                expect_token_kind(lexer, TokenKindSet::single(TokenKind::Equals))?;
                let body = Expr::parse_peekable(lexer)?;
                let rule = Rule {
                    loc: keyword.loc,
                    head,
                    body,
                };
                println!("Defined rule {}", &rule);
                self.rules.insert(name.text, rule);
            }
            TokenKind::Shape => {
                if let Some(_) = self.current_expr {
                    return Err(Error::AlreadyShaping)
                }

                let expr = Expr::parse_peekable(lexer)?;
                println!("Shaping {}", &expr);
                self.current_expr = Some(expr);
            },
            TokenKind::Apply => {
                if let Some(expr) = &self.current_expr {
                    let name = expect_token_kind(lexer, TokenKindSet::single(TokenKind::Sym))?;
                    if let Some(rule) = self.rules.get(&name.text) {
                        let new_expr = rule.apply_all(&expr);
                        println!("{}", &new_expr);
                        self.current_expr = Some(new_expr);
                    } else {
                        return Err(Error::RuleDoesNotExist(name.text));
                    }
                } else {
                    return Err(Error::NoShapingInPlace);
                }
            }
            TokenKind::Done => {
                if let Some(expr) = &self.current_expr {
                    println!("Finished shaping expression {}", expr);
                    self.current_expr = None
                } else {
                    return Err(Error::NoShapingInPlace)
                }
            }
            _ => unreachable!("Expected {} but got {}", expected_tokens, keyword.kind),
        }
        Ok(())
    }
}

fn main() {
    let mut context = Context::default();
    let mut command = String::new();

    let prompt = "> ";

    loop {
        command.clear();
        print!("{}", prompt);
        stdout().flush().unwrap();
        stdin().read_line(&mut command).unwrap();
        let mut lexer = Lexer::from_iter(command.chars()).peekable();
        let result = context.process_command(&mut lexer)
            .and_then(|()| expect_token_kind(&mut lexer, TokenKindSet::single(TokenKind::End)));
        match result {
            Err(Error::UnexpectedToken(expected, actual)) => {
                eprintln!("{:>width$}^", "", width=prompt.len() + actual.loc.col);
                eprintln!("ERROR: expected {} but got {} '{}'", expected, actual.kind, actual.text);
            }
            Err(Error::RuleAlreadyExists(name, new_loc, _old_loc)) => {
                eprintln!("{:>width$}^", "", width=prompt.len() + new_loc.col);
                eprintln!("ERROR: redefinition of existing rule {}", name);
            }
            Err(Error::AlreadyShaping) => {
                eprintln!("ERROR: already shaping an expression. Finish the current shaping with {} first.",
                          TokenKind::Done);
            }
            Err(Error::NoShapingInPlace) => {
                eprintln!("ERROR: no shaping in place.");
            }
            Err(Error::RuleDoesNotExist(name)) => {
                eprintln!("ERROR: rule {} does not exist", name);
            }
            Ok(_) => {}
        }
    }
}
