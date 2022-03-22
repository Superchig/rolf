use rolf_parser::parser::{lex_overall, parse_overall, Parser, Scanner};

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
    println!("{}: {:#?}", input, lex_overall(&mut Scanner::new(input)));
}

fn test_parse(input: &str) {
    match lex_overall(&mut Scanner::new(input)) {
        Ok(tokens) => {
            println!("{}: {:?}", input, parse_overall(&mut Parser::new(tokens)));
        }
        Err(err) => eprintln!("{} - error: {:?}", input, err),
    }
}
