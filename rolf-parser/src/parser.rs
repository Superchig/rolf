use core::fmt;
use std::{error::Error, mem, ops};

type LexResult<T> = std::result::Result<T, LexError>;
type ParseResult<T> = std::result::Result<T, ParseError>;

pub fn parse(input: &str) -> ParseResult<Program> {
    parse_rule_from(input, parse_overall)
}

pub fn parse_statement_from(input: &str) -> ParseResult<Statement> {
    parse_rule_from(input, parse_statement)
}

pub fn parse_rule_from<T>(input: &str, parse_rule: impl Fn(&mut Parser) -> ParseResult<T>) -> ParseResult<T> {
    let mut scanner = Scanner::new(input);

    match lex_overall(&mut scanner) {
        Ok(tokens) => parse_rule(&mut Parser::new(tokens)),
        Err(err) => match scanner.peek() {
            Some(_) => Err(ParseError {
                position: Position::Pos {
                    line: scanner.curr_line,
                    col: scanner.curr_col,
                },
                kind: ParseErrorKind::LexError(err),
            }),
            None => Err(ParseError::new(ParseErrorKind::LexError(err))),
        },
    }
}

pub fn lex_overall(scanner: &mut Scanner) -> LexResult<Vec<Token>> {
    let lex_map = lex_phrase("map");
    let lex_plus = lex_phrase("+");

    // NOTE(Chris): The order matters here, in case one lexing rule conflicts with another.
    let mut lexers: Vec<&dyn Fn(&mut Scanner) -> LexResult<Token>> =
        vec![&lex_mod, &lex_newline, &lex_whitespace, &lex_map, &lex_plus];

    lexers.push(&lex_id);

    let mut tokens = vec![];

    let mut prev_line = 1;
    let mut prev_col = 1;
    'scanner: while !scanner.is_done() {
        for lexer in &lexers {
            if let Ok(mut token) = lexer(scanner) {
                // Move the line and column numbers "back" for each token, so that they contain their starting
                // positions rather than their ending positions.
                mem::swap(&mut token.line, &mut prev_line);
                mem::swap(&mut token.col, &mut prev_col);

                // Ignore whitespace
                if token.kind != TokenKind::Whitespace {
                    tokens.push(token);
                }

                continue 'scanner;
            }
        }

        return Err(LexError::RemainingInput);
    }

    Ok(tokens)
}

fn lex_id(scanner: &mut Scanner) -> LexResult<Token> {
    let mut buf = String::new();

    loop {
        let lowercase = scanner.pop_in_range('a'..='z');

        if let Some(letter) = lowercase {
            buf.push(letter);
            continue;
        }

        let uppercase = scanner.pop_in_range('a'..='z');

        if let Some(letter) = uppercase {
            buf.push(letter);
            continue;
        }

        if scanner.take(&'-') {
            buf.push('-');
            continue;
        }

        break;
    }

    if buf.is_empty() {
        Err(LexError::ExpectedId)
    } else {
        Ok(Token::new(scanner, TokenKind::Id(buf)))
    }
}

fn lex_mod(scanner: &mut Scanner) -> LexResult<Token> {
    if scanner.take_str("ctrl") {
        Ok(Token::new(scanner, TokenKind::Mod(Mod::Ctrl)))
    } else if scanner.take_str("shift") {
        Ok(Token::new(scanner, TokenKind::Mod(Mod::Shift)))
    } else if scanner.take_str("alt") {
        Ok(Token::new(scanner, TokenKind::Mod(Mod::Alt)))
    } else {
        Err(LexError::ExpectedMod)
    }
}

fn lex_phrase(phrase: &'static str) -> Box<dyn Fn(&mut Scanner) -> LexResult<Token>> {
    Box::new(move |scanner: &mut Scanner| {
        if scanner.take_str(phrase) {
            Ok(Token::new(scanner, TokenKind::Phrase(phrase)))
        } else {
            Err(LexError::ExpectedPhrase(phrase))
        }
    })
}

fn lex_whitespace(scanner: &mut Scanner) -> LexResult<Token> {
    let mut was_whitespace = false;

    while let Some(_ch) = scanner.pop_in_slice(&[' ', '\t']) {
        was_whitespace = true;
    }

    if was_whitespace {
        Ok(Token::new(scanner, TokenKind::Whitespace))
    } else {
        Err(LexError::ExpectedWhitespace)
    }
}

fn lex_newline(scanner: &mut Scanner) -> LexResult<Token> {
    if scanner.take(&'\n') {
        Ok(Token::new(scanner, TokenKind::Newline))
    } else {
        Err(LexError::ExpectedNewline)
    }
}

#[derive(Debug)]
pub struct Token {
    line: usize,
    col: usize,
    kind: TokenKind,
}

impl Token {
    pub fn new(scanner: &Scanner, kind: TokenKind) -> Self {
        Token {
            line: scanner.curr_line,
            col: scanner.curr_col,
            kind,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TokenKind {
    Id(String),
    Mod(Mod),
    Phrase(&'static str),
    Whitespace,
    Newline,
}

pub fn parse_overall(parser: &mut Parser) -> ParseResult<Program> {
    let result = parse_program(parser)?;

    if parser.is_done() {
        Ok(result)
    } else {
        Err(ParseError::new_pos(
            parser.peek().unwrap(),
            ParseErrorKind::RemainingTokens,
        ))
    }
}

fn parse_program(parser: &mut Parser) -> ParseResult<Program> {
    parser.take_list(Some(TokenKind::Newline), parse_statement)
}

pub fn parse_statement(parser: &mut Parser) -> ParseResult<Statement> {
    if let Ok(map) = parse_map(parser) {
        Ok(Statement::Map(map))
    } else if let Ok(command_use) = parse_command_use(parser) {
        Ok(Statement::CommandUse(command_use))
    } else {
        match parser.peek() {
            Some(token) => Err(ParseError::new_pos(token, ParseErrorKind::ExpectedList)),
            None => Err(ParseError::new(ParseErrorKind::ExpectedList)),
        }
    }
}

fn parse_command_use(parser: &mut Parser) -> ParseResult<CommandUse> {
    let name = parser.take_id()?;

    let arguments = parser.take_list(None, |parser| parser.take_id())?;

    Ok(CommandUse { name, arguments })
}

fn parse_map(parser: &mut Parser) -> ParseResult<Map> {
    parser.expect(TokenKind::Phrase("map"))?;

    let key = parse_key(parser)?;

    let cmd_name = parser.take_id()?;

    Ok(Map { key, cmd_name })
}

fn parse_key(parser: &mut Parser) -> ParseResult<Key> {
    let modifier = match parser.take_mod() {
        Ok(modifier) => {
            parser.expect(TokenKind::Phrase("+"))?;

            Some(modifier)
        }
        Err(_) => None,
    };

    let key = parser.take_id()?;

    Ok(Key { key, modifier })
}

pub type Program = Vec<Statement>;

#[derive(Debug, Clone)]
pub enum Statement {
    Map(Map),
    CommandUse(CommandUse),
}

#[derive(Debug, Clone)]
pub struct CommandUse {
    pub name: String,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Map {
    pub key: Key,
    pub cmd_name: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Key {
    pub modifier: Option<Mod>,
    pub key: String,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Mod {
    Ctrl,
    Shift,
    Alt,
}

#[derive(Debug)]
pub struct Parser {
    cursor: usize,
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { cursor: 0, tokens }
    }

    /// Returns the current cursor. Useful for reporting errors.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the next character without advancing the cursor.
    /// AKA "lookahead"
    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    /// Returns true if further progress is not possible
    pub fn is_done(&self) -> bool {
        self.cursor >= self.tokens.len()
    }

    /// Returns the next character (if available) and advances the cursor.
    pub fn pop(&mut self) -> Option<&Token> {
        match self.tokens.get(self.cursor) {
            Some(token) => {
                self.cursor += 1;

                Some(token)
            }
            None => None,
        }
    }

    pub fn take_id(&mut self) -> ParseResult<String> {
        match self.peek() {
            Some(Token {
                kind: TokenKind::Id(name),
                ..
            }) => {
                let copy = name.clone();

                self.pop();

                Ok(copy)
            }
            Some(token) => Err(ParseError::new_pos(token, ParseErrorKind::ExpectedId)),
            None => Err(ParseError::new(ParseErrorKind::ExpectedId)),
        }
    }

    pub fn take_mod(&mut self) -> ParseResult<Mod> {
        match self.peek() {
            Some(Token {
                kind: TokenKind::Mod(mod_enum),
                ..
            }) => {
                let copy = *mod_enum;

                self.pop();

                Ok(copy)
            }
            Some(token) => Err(ParseError::new_pos(token, ParseErrorKind::ExpectedMod)),
            None => Err(ParseError::new(ParseErrorKind::ExpectedMod)),
        }
    }

    /// Returns Some(()) if the `target` is found at the current cursor position, and advances the
    /// cursor.
    /// Otherwise, returns None, leaving the cursor unchanged.
    pub fn expect(&mut self, target: TokenKind) -> ParseResult<()> {
        match self.tokens.get(self.cursor) {
            Some(token) => {
                if target == token.kind {
                    self.pop();

                    Ok(())
                } else {
                    Err(ParseError::new_pos(token, ParseErrorKind::Expected(target)))
                }
            }
            None => Err(ParseError::new(ParseErrorKind::Expected(target))),
        }
    }

    pub fn take_list<T>(
        &mut self,
        separator: Option<TokenKind>,
        parse_cb: impl Fn(&mut Parser) -> ParseResult<T>,
    ) -> ParseResult<Vec<T>> {
        let mut list = vec![];

        while let Ok(item) = parse_cb(self) {
            list.push(item);

            if let Some(ref sep) = separator {
                match self.peek() {
                    Some(token) => {
                        if &token.kind == sep {
                            self.pop();
                        } else {
                            return Err(ParseError::new_pos(
                                token,
                                ParseErrorKind::Expected(sep.clone()),
                            ));
                        }
                    }
                    None => break,
                }
            }
        }

        Ok(list)
    }
}

// FIXME(Chris): Remove dead code ignores
#[derive(Debug)]
#[allow(dead_code)]
pub struct ParseError {
    position: Position,
    kind: ParseErrorKind,
}

#[derive(Debug)]
pub enum Position {
    EOF,
    Pos { line: usize, col: usize },
}

#[derive(Debug)]
pub enum ParseErrorKind {
    Message(String),
    RemainingTokens,
    Expected(TokenKind),
    ExpectedId,
    ExpectedMod,
    ExpectedEof,
    ExpectedList,
    LexError(LexError),
}

impl ParseError {
    fn new(kind: ParseErrorKind) -> Self {
        Self {
            position: Position::EOF,
            kind,
        }
    }

    fn new_pos(token: &Token, kind: ParseErrorKind) -> Self {
        Self {
            position: Position::Pos {
                line: token.line,
                col: token.col,
            },
            kind,
        }
    }
}

pub struct Scanner {
    cursor: usize,
    characters: Vec<char>,
    curr_line: usize,
    curr_col: usize,
}

impl Scanner {
    pub fn new(string: &str) -> Self {
        Self {
            cursor: 0,
            characters: string.chars().collect(),
            // Files start at line 1, column 1
            curr_line: 1,
            curr_col: 1,
        }
    }

    /// Returns the current cursor. Useful for reporting errors.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the next character without advancing the cursor.
    /// AKA "lookahead"
    pub fn peek(&self) -> Option<&char> {
        self.characters.get(self.cursor)
    }

    /// Returns true if further progress is not possible
    pub fn is_done(&self) -> bool {
        self.cursor >= self.characters.len()
    }

    /// Returns the next character (if available) and advances the cursor.
    pub fn pop(&mut self) -> Option<&char> {
        match self.characters.get(self.cursor) {
            Some(character) => {
                if character == &'\n' {
                    self.curr_line += 1;
                    self.curr_col = 1;
                } else {
                    self.curr_col += 1;
                }

                self.cursor += 1;

                Some(character)
            }
            None => None,
        }
    }

    /// Returns the next character if it's in the given range, and advances the cursor.
    /// Otherwise, returns None, leaving the cursor unchanged.
    pub fn pop_in_range(&mut self, target_range: ops::RangeInclusive<char>) -> Option<char> {
        match self.peek() {
            Some(ch) => {
                if target_range.contains(ch) {
                    let copy = *ch;

                    self.pop();

                    Some(copy)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Returns the next character if it's in the given slice, and advances the cursor.
    /// Otherwise, returns None, leaving the cursor unchanged.
    pub fn pop_in_slice(&mut self, range_slice: &[char]) -> Option<char> {
        match self.peek() {
            Some(ch) => {
                if range_slice.contains(ch) {
                    let copy = *ch;

                    self.pop();

                    Some(copy)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Returns true if the `target` is found at the current cursor position,
    /// and advances the cursor.
    /// Otherwise, returns false, leaving the cursor unchanged.
    pub fn take(&mut self, target: &char) -> bool {
        match self.characters.get(self.cursor) {
            Some(character) => {
                if target == character {
                    self.pop();

                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }

    /// Returns Some(()) if the `target` is found at the current cursor position, and advances the
    /// cursor.
    /// Otherwise, returns None, leaving the cursor unchanged.
    pub fn expect(&mut self, target: &char) -> LexResult<()> {
        match self.characters.get(self.cursor) {
            Some(character) => {
                if target == character {
                    self.pop();

                    Ok(())
                } else {
                    Err(LexError::Expected(*target))
                }
            }
            None => Err(LexError::Expected(*target)),
        }
    }

    pub fn take_str(&mut self, target: &str) -> bool {
        if target.len() + self.cursor > self.characters.len() {
            return false;
        }

        let mut ind = self.cursor;

        for ch in target.chars() {
            if ch != self.characters[ind] {
                return false;
            }

            ind += 1;
        }

        let orig_cursor = self.cursor;

        for _ in orig_cursor..ind {
            self.pop();
        }

        true
    }

    /// Invoke `cb` once. If the result is not `None`, return it and advance
    /// the cursor. Otherwise, return None and leave the cursor unchanged.
    pub fn transform<T>(&mut self, cb: impl FnOnce(&char) -> Option<T>) -> Option<T> {
        match self.characters.get(self.cursor) {
            Some(input) => match cb(input) {
                Some(output) => {
                    self.pop();

                    Some(output)
                }
                None => None,
            },
            None => None,
        }
    }
}

#[derive(Debug)]
pub enum LexError {
    Expected(char),
    ExpectedPhrase(&'static str),
    ExpectedDigit,
    ExpectedLetter,
    ExpectedId,
    ExpectedMod,
    ExpectedWhitespace,
    ExpectedNewline,
    RemainingInput,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for LexError {}
