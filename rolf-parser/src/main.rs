use rolf_parser::parser::{lex, parse, Parser, Scanner};

fn main() {
    test_lex("ctrl");
    // test_lex("a");
    // test_lex("-");
    // test_lex("abra");
    // test_lex("map ctrl");
    // test_lex("map ctrl+a");
    // test_lex("map ctrl+k up");

    println!();

    // test_parse("map ctrl+k"); // This should result in an ExpectedId error
    // test_parse("map ctrl+k up\nmap j down");
    // test_parse("map up up\nmap down down");
    test_parse("down\nup again");
}

fn test_lex(input: &str) {
    println!("{}: {:#?}", input, lex(&mut Scanner::new(input)));
}

fn test_parse(input: &str) {
    match lex(&mut Scanner::new(input)) {
        Ok(tokens) => {
            println!("{}: {:?}", input, parse(&mut Parser::new(tokens)));
        }
        Err(err) => eprintln!("{} - error: {:?}", input, err),
    }
}
