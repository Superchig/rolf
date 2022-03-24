use std::io;

use crossterm::event::{self, Event, KeyCode};
use rolf_grid::{Attribute, Screen, Style};

fn main() -> io::Result<()> {
    let mut screen = Screen::new(io::stdout())?;

    screen.activate()?;

    let mut x = 1;
    let mut y = 1;

    let mut is_cursor_visible = true;

    let (width, height) = screen.size();

    loop {
        screen.set_cell(0, 0, '┌');
        screen.set_cell(width - 1, 0, '┐');
        screen.set_cell(0, height - 1, '└');
        screen.set_cell(width - 1, height - 1, '┘');

        for y in 1..height - 1 {
            screen.set_cell(0, y, '│');
            screen.set_cell(width - 1, y, '│');
        }

        for x in 1..width - 1 {
            screen.set_cell(x, 0, '─');
            screen.set_cell(x, height - 1, '─');
        }

        if is_cursor_visible {
            screen.show_cursor(x, y);
        } else {
            screen.hide_cursor();
        }
        draw_str(&mut screen, 10, 1, "Welcome!", Style::new(Attribute::Bold));
        draw_str(&mut screen, 10, 3, "This is underlined.", Style::new(Attribute::Underlined));
        screen.set_cell(x, y, '@');
        screen.show()?;

        screen.set_cell(x, y, ' ');

        let event = event::read()?;

        match event {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Char('j') => {
                    if y < height - 1 {
                        y += 1;
                    }
                }
                KeyCode::Char('k') => {
                    if y > 0 {
                        y -= 1;
                    }
                }
                KeyCode::Char('l') => {
                    if x < width - 1 {
                        x += 1;
                    }
                }
                KeyCode::Char('h') => {
                    if x > 0 {
                        x -= 1;
                    }
                }
                KeyCode::Char('q') => break,
                KeyCode::Char('s') => {
                    is_cursor_visible = !is_cursor_visible;
                }
                _ => (),
            },
            Event::Mouse(..) => (),
            Event::Resize(_, _) => (),
        }
    }

    screen.deactivate()?;

    Ok(())
}

fn draw_str(screen: &mut Screen<io::Stdout>, x: u16, y: u16, string: &str, style: Style) {
    for (i, ch) in string.char_indices() {
        let i: u16 = i.try_into().expect("Should be able to fit into a u16.");
        screen.set_cell_style(x + i, y, ch, style);
    }
}
