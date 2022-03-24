use std::io;

use crossterm::event::{self, Event, KeyCode};
use rolf_grid::Screen;

fn main() -> io::Result<()> {
    let mut screen = Screen::new(io::stdout())?;

    screen.activate()?;

    let x = 10;
    let mut y = 10;

    screen.set_cell(0, 0, '┌');

    for y in 1..=20 {
        screen.set_cell(0, y, '│');
    }

    loop {
        screen.show_cursor(x, y);
        screen.set_cell(x, y, '@');
        screen.show()?;

        screen.set_cell(x, y, ' ');

        let event = event::read()?;

        match event {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Char('j') => {
                    y = clamp(y + 1, 0, 20);
                }
                KeyCode::Char('k') => {
                    y = clamp(y - 1, 0, 20);
                }
                KeyCode::Char('q') => break,
                _ => (),
            },
            Event::Mouse(..) => (),
            Event::Resize(_, _) => (),
        }
    }

    screen.deactivate()?;

    Ok(())
}

fn clamp<T>(value: T, min: T, max: T) -> T
where
    T: PartialOrd<T>,
{
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
