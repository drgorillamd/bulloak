use std::{borrow::Borrow, cell::Cell, result};

use crate::{
    ast::{Action, Ast, Condition, Root},
    span::Span,
    tokenizer::{Token, TokenKind},
};
use std::fmt;

type Result<T> = result::Result<T, Error>;

/// An error that occurred while parsing a sequence of tokens into an abstract
/// syntax tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    /// The kind of error.
    kind: ErrorKind,
    /// The original text that the parser generated the error from. Every
    /// span in an error is a valid range into this string.
    text: String,
    /// The span of this error.
    span: Span,
}

impl Error {
    /// Return the type of this error.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// The original text string in which this error occurred.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return the span at which this error occurred.
    pub fn span(&self) -> &Span {
        &self.span
    }
}

/// The type of an error that occurred while building an AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    /// This might happen because of an internal bug or the user
    /// might have passed an invalid .tree.
    /// An example of how this might be an internal bug is if the
    /// parser ends up in a state where the current grammar production
    /// being applied doesn't expect this token to occur.
    UnexpectedToken,
    /// Did not expect this WHEN keyword.
    UnexpectedWhen,
    /// Did not expect this IT keyword.
    UnexpectedIt,
    /// Did not expect a STRING.
    UnexpectedString,
    /// Did not expect an end of file.
    UnexpectedEof,
    /// This enum may grow additional variants, so this makes sure clients
    /// don't count on exhaustive matching. (Otherwise, adding a new variant
    /// could break existing code.)
    #[doc(hidden)]
    __Nonexhaustive,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::error::Formatter::from(self).fmt(f)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::ErrorKind::*;
        match *self {
            UnexpectedToken => write!(f, "unexpected token"),
            UnexpectedWhen => write!(f, "unexpected WHEN keyword"),
            UnexpectedIt => write!(f, "unexpected IT keyword"),
            UnexpectedString => write!(f, "unexpected STRING"),
            UnexpectedEof => write!(f, "unexpected end of file"),
            _ => unreachable!(),
        }
    }
}

pub struct Parser {
    current: Cell<usize>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            current: Cell::new(0),
        }
    }

    pub fn parse(&mut self, text: &str, tokens: &[Token]) -> Result<Ast> {
        ParserI::new(self, text, tokens).parse()
    }

    fn reset(&self) {
        self.current.set(0);
    }
}

struct ParserI<'t, P> {
    text: &'t str,
    tokens: &'t [Token],
    parser: P,
}

impl<'t, P: Borrow<Parser>> ParserI<'t, P> {
    fn new(parser: P, text: &'t str, tokens: &'t [Token]) -> Self {
        Self {
            text,
            tokens,
            parser,
        }
    }

    fn parser(&self) -> &Parser {
        self.parser.borrow()
    }

    /// Create a new error with the given span and error type.
    fn error(&self, span: Span, kind: ErrorKind) -> Error {
        Error {
            kind,
            text: self.text.to_string(),
            span,
        }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.parser().current.get())
    }

    fn previous(&self) -> Option<&Token> {
        if self.parser().current.get() == 0 {
            return None;
        }
        self.tokens.get(self.parser().current.get() - 1)
    }

    fn consume(&self) -> Option<&Token> {
        if self.parser().current.get() + 1 > self.tokens.len() {
            return None;
        }
        self.parser().current.set(self.parser().current.get() + 1);
        self.tokens.get(self.parser().current.get())
    }

    pub fn parse(&self) -> Result<Ast> {
        self.parser().reset();
        self._parse()
    }

    fn _parse(&self) -> Result<Ast> {
        let current_token = match self.current() {
            Some(current) => current,
            None => {
                return Err(self
                    .error(self.tokens.last().unwrap().span, ErrorKind::UnexpectedEof)
                    .into())
            }
        };

        match current_token.kind {
            TokenKind::STRING if self.parser().current.get() == 0 => self.parse_root(current_token),
            TokenKind::TEE | TokenKind::CORNER => {
                let next_token = match self.consume() {
                    Some(next) => next,
                    None => {
                        return Err(self
                            .error(self.tokens.last().unwrap().span, ErrorKind::UnexpectedEof)
                            .into())
                    }
                };

                match next_token.kind {
                    TokenKind::IT => {
                        let title = self.parse_string(next_token);
                        let previous = self.previous().unwrap();
                        Ok(Ast::Action(Action {
                            title,
                            span: Span::new(current_token.span.start, previous.span.end),
                        }))
                    }
                    TokenKind::WHEN => {
                        let title = self.parse_string(next_token);

                        let mut asts = vec![];
                        while self
                            .current()
                            // Only parse tokens that are indented more than the current token.
                            // The column is our way to determine which tree level are we in.
                            .is_some_and(|t| t.span.start.column > current_token.span.start.column)
                        {
                            let ast = self._parse()?;
                            asts.push(ast);
                        }

                        let previous = self.previous().unwrap();
                        Ok(Ast::Condition(Condition {
                            title,
                            asts,
                            span: Span::new(current_token.span.start, previous.span.end),
                        }))
                    }
                    _ => Err(self
                        .error(current_token.span, ErrorKind::UnexpectedToken)
                        .into()),
                }
            }
            TokenKind::STRING => Err(self
                .error(current_token.span, ErrorKind::UnexpectedString)
                .into()),
            TokenKind::WHEN => Err(self
                .error(current_token.span, ErrorKind::UnexpectedWhen)
                .into()),
            TokenKind::IT => Err(self
                .error(current_token.span, ErrorKind::UnexpectedIt)
                .into()),
        }
    }

    fn parse_root(&self, current_token: &Token) -> Result<Ast> {
        self.consume();
        // A string at the start of the file is the root ast node.
        let mut asts = vec![];
        while self.current().is_some() {
            let ast = self._parse()?;
            asts.push(ast);
        }

        let last_span = if asts.len() > 0 {
            asts[asts.len() - 1].span()
        } else {
            &current_token.span
        };

        Ok(Ast::Root(Root {
            span: Span::new(current_token.span.start, last_span.end),
            asts,
            file_name: current_token.lexeme.clone(),
        }))
    }

    fn parse_string(&self, start_token: &Token) -> String {
        // Strings always start with one of IT or WHEN.
        let mut string = String::from(&start_token.lexeme);

        // Consume all words.
        loop {
            match self.consume() {
                Some(token) => match token.kind {
                    TokenKind::STRING | TokenKind::IT | TokenKind::WHEN => {
                        string = string + " " + &token.lexeme;
                    }
                    _ => break,
                },
                None => break,
            }
        }

        string
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::ast::{Action, Ast, Condition, Root};
    use crate::error::Result;
    use crate::parser::Parser;
    use crate::tokenizer::Tokenizer;
    use crate::{
        span::{Position, Span},
        tokenizer::{Token, TokenKind},
    };

    #[test]
    fn test_only_filename() -> Result<()> {
        let file_contents = String::from("foo");
        let tokens = vec![Token {
            kind: TokenKind::STRING,
            lexeme: String::from("foo"),
            span: Span::new(Position::new(0, 1, 1), Position::new(2, 1, 3)),
        }];
        let ast = Parser::new().parse(&file_contents, &tokens)?;

        assert_eq!(
            ast,
            Ast::Root(Root {
                span: Span::new(Position::new(0, 1, 1), Position::new(2, 1, 3)),
                asts: vec![],
                file_name: String::from("foo"),
            })
        );

        Ok(())
    }

    #[test]
    fn test_one_child() -> Result<()> {
        // TODO: Setup tokens by hand instead of relying on the tokenizer.
        let file_contents =
            String::from("file.sol\n└── when something bad happens\n   └── it should revert");

        // Token("file.sol", Span(Position(o: 0, l: 1, c: 1), Position(o: 7, l: 1, c: 8))),
        // Token("└", Span(Position(o: 9, l: 2, c: 1), Position(o: 9, l: 2, c: 1))),
        // Token("when", Span(Position(o: 19, l: 2, c: 5), Position(o: 22, l: 2, c: 8))),
        // Token("something", Span(Position(o: 24, l: 2, c: 10), Position(o: 32, l: 2, c: 18))),
        // Token("bad", Span(Position(o: 34, l: 2, c: 20), Position(o: 36, l: 2, c: 22))),
        // Token("happens", Span(Position(o: 38, l: 2, c: 24), Position(o: 44, l: 2, c: 30))),
        // Token("└", Span(Position(o: 50, l: 3, c: 5), Position(o: 50, l: 3, c: 5))),
        // Token("it", Span(Position(o: 60, l: 3, c: 9), Position(o: 61, l: 3, c: 10))),
        // Token("should", Span(Position(o: 63, l: 3, c: 12), Position(o: 68, l: 3, c: 17))),
        // Token("revert", Span(Position(o: 70, l: 3, c: 19), Position(o: 75, l: 3, c: 24))),
        let tokens = Tokenizer::new().tokenize(&file_contents)?;
        let ast = Parser::new().parse(&file_contents, &tokens)?;

        assert_eq!(
            ast,
            Ast::Root(Root {
                span: Span::new(Position::new(0, 1, 1), Position::new(74, 3, 23)),
                asts: vec![Ast::Condition(Condition {
                    span: Span::new(Position::new(9, 2, 1), Position::new(74, 3, 23)),
                    title: String::from("when something bad happens"),
                    asts: vec![Ast::Action(Action {
                        span: Span::new(Position::new(49, 3, 4), Position::new(74, 3, 23)),
                        title: String::from("it should revert"),
                    })],
                })],
                file_name: String::from("file.sol"),
            })
        );

        Ok(())
    }

    #[test]
    fn test_two_children() -> Result<()> {
        // TODO: Setup tokens by hand instead of relying on the tokenizer.
        let file_contents = String::from(
            r#"two_children.t.sol
├── when stuff called
│  └── it should revert
└── when not stuff called
   └── it should revert"#,
        );

        // Token("two_children.t.sol", Span(Position(o: 0, l: 1, c: 1), Position(o: 17, l: 1, c: 18))),
        // Token("├", Span(Position(o: 19, l: 2, c: 1), Position(o: 19, l: 2, c: 1))),
        // Token("when", Span(Position(o: 29, l: 2, c: 5), Position(o: 32, l: 2, c: 8))),
        // Token("stuff", Span(Position(o: 34, l: 2, c: 10), Position(o: 38, l: 2, c: 14))),
        // Token("called", Span(Position(o: 40, l: 2, c: 16), Position(o: 45, l: 2, c: 21))),
        // Token("└", Span(Position(o: 52, l: 3, c: 4), Position(o: 52, l: 3, c: 4))),
        // Token("it", Span(Position(o: 62, l: 3, c: 8), Position(o: 63, l: 3, c: 9))),
        // Token("should", Span(Position(o: 65, l: 3, c: 11), Position(o: 70, l: 3, c: 16))),
        // Token("revert", Span(Position(o: 72, l: 3, c: 18), Position(o: 77, l: 3, c: 23))),
        // Token("└", Span(Position(o: 79, l: 4, c: 1), Position(o: 79, l: 4, c: 1))),
        // Token("when", Span(Position(o: 89, l: 4, c: 5), Position(o: 92, l: 4, c: 8))),
        // Token("not", Span(Position(o: 94, l: 4, c: 10), Position(o: 96, l: 4, c: 12))),
        // Token("stuff", Span(Position(o: 98, l: 4, c: 14), Position(o: 102, l: 4, c: 18))),
        // Token("called", Span(Position(o: 104, l: 4, c: 20), Position(o: 109, l: 4, c: 25))),
        // Token("└", Span(Position(o: 114, l: 5, c: 4), Position(o: 114, l: 5, c: 4))),
        // Token("it", Span(Position(o: 124, l: 5, c: 8), Position(o: 125, l: 5, c: 9))),
        // Token("should", Span(Position(o: 127, l: 5, c: 11), Position(o: 132, l: 5, c: 16))),
        // Token("revert", Span(Position(o: 134, l: 5, c: 18), Position(o: 139, l: 5, c: 23))),
        let tokens = Tokenizer::new().tokenize(&file_contents)?;
        let ast = Parser::new().parse(&file_contents, &tokens)?;

        assert_eq!(
            ast,
            Ast::Root(Root {
                file_name: String::from("two_children.t.sol"),
                span: Span::new(Position::new(0, 1, 1), Position::new(139, 5, 23)),
                asts: vec![
                    Ast::Condition(Condition {
                        title: String::from("when stuff called"),
                        span: Span::new(Position::new(19, 2, 1), Position::new(77, 3, 23)),
                        asts: vec![Ast::Action(Action {
                            title: String::from("it should revert"),
                            span: Span::new(Position::new(52, 3, 4), Position::new(77, 3, 23)),
                        })],
                    }),
                    Ast::Condition(Condition {
                        title: String::from("when not stuff called"),
                        span: Span::new(Position::new(79, 4, 1), Position::new(139, 5, 23)),
                        asts: vec![Ast::Action(Action {
                            title: String::from("it should revert"),
                            span: Span::new(Position::new(114, 5, 4), Position::new(139, 5, 23)),
                        })],
                    }),
                ],
            })
        );

        Ok(())
    }
}