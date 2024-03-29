use io::Stdout;

use crossterm::{
    cursor, execute, queue, style,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Write},
    ops::{BitAnd, BitOr, BitOrAssign},
};

pub struct Screen<T>
where
    T: Write,
{
    output: T,
    output_buf: Vec<u8>,
    grid: Grid<Cell>,
    prev_grid: Grid<Cell>,
    cursor_display: (u16, u16),
    should_show_cursor: bool,
    last_style: Style,
}

impl<T> Screen<T>
where
    T: Write,
{
    pub fn new(screen_output: T) -> io::Result<Self> {
        let (width, height) = terminal::size()?;

        Ok(Self {
            output: screen_output,
            output_buf: vec![],
            grid: Grid::new(width, height),
            prev_grid: Grid::new(width, height),
            cursor_display: (0, 0),
            should_show_cursor: false,
            last_style: Style::default(),
        })
    }

    pub fn show_cursor(&mut self, x: u16, y: u16) {
        self.cursor_display = (x, y);
        self.should_show_cursor = true;
    }

    pub fn hide_cursor(&mut self) {
        self.should_show_cursor = false;
    }

    pub fn set_cell(&mut self, x: u16, y: u16, ch: char) {
        self.set_cell_style(x, y, ch, Style::default());
    }

    pub fn set_cell_style(&mut self, x: u16, y: u16, ch: char, style: Style) {
        let mut cell = self.grid.get_mut(x, y);
        cell.ch = ch;
        cell.style = style;
    }

    pub fn activate_direct(output: &mut T) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(output, EnterAlternateScreen)?;

        Ok(())
    }

    pub fn deactivate_direct(output: &mut T) -> io::Result<()> {
        terminal::disable_raw_mode()?;
        execute!(output, LeaveAlternateScreen, cursor::Show)?;

        Ok(())
    }

    pub fn activate(&mut self) -> io::Result<()> {
        Self::activate_direct(&mut self.output)
    }

    pub fn deactivate(&mut self) -> io::Result<()> {
        Self::deactivate_direct(&mut self.output)
    }

    pub fn size(&self) -> (u16, u16) {
        (self.grid.width, self.grid.height)
    }

    pub fn clear_logical(&mut self) {
        Self::clear_grid(&mut self.grid);
    }

    fn clear_grid(grid: &mut Grid<Cell>) {
        for x in 0..grid.width {
            for y in 0..grid.height {
                let mut cell = grid.get_mut(x, y);
                cell.ch = ' ';
                cell.style = Style::default();
            }
        }
    }

    pub fn resize_clear_draw(&mut self, width: u16, height: u16) -> io::Result<()> {
        self.prev_grid.resize_blunt(width, height, Cell::default());
        self.grid.resize_blunt(width, height, Cell::default());

        Self::clear_grid(&mut self.prev_grid);
        Self::clear_grid(&mut self.grid);

        self.last_style = Style::default();

        execute!(
            &mut self.output,
            style::SetAttribute(style::Attribute::Reset),
            terminal::Clear(ClearType::All)
        )?;

        Ok(())
    }

    pub fn build_line(&mut self, x: u16, y: u16, builder: &LineBuilder) {
        let mut curr_x = x;

        for cell in &builder.cells {
            if curr_x >= self.grid.width {
                break;
            }

            let grid_cell = self.grid.get_mut(curr_x, y);
            *grid_cell = *cell;

            curr_x += 1;
        }
    }

    pub fn set_dead(&mut self, x: u16, y: u16, is_dead: bool) {
        let mut cell = self.grid.get_mut(x, y);
        cell.is_dead = is_dead;
    }
}

impl Screen<Stdout> {
    pub fn show(&mut self) -> io::Result<()> {
        let mut stdout_lock = self.output.lock();

        for x in 0..self.grid.width {
            for y in 0..self.grid.height {
                let cell = self.grid.get(x, y);
                let prev_cell = self.prev_grid.get(x, y);

                if cell != prev_cell && !cell.is_dead {
                    if cell.style != self.last_style {
                        queue!(
                            &mut self.output_buf,
                            style::SetAttribute(style::Attribute::Reset),
                        )?;

                        cell.style.attribute.queue_crossterm(&mut self.output_buf)?;

                        if cell.style.fg != Color::Foreground && cell.style.bg != Color::Background
                        {
                            queue!(
                                &mut self.output_buf,
                                style::SetColors(style::Colors::new(
                                    cell.style.fg.to_crossterm(),
                                    cell.style.bg.to_crossterm()
                                )),
                            )?;
                        } else if cell.style.bg != Color::Background {
                            queue!(
                                &mut self.output_buf,
                                style::SetBackgroundColor(cell.style.bg.to_crossterm()),
                            )?;
                        } else if cell.style.fg != Color::Foreground {
                            queue!(
                                &mut self.output_buf,
                                style::SetForegroundColor(cell.style.fg.to_crossterm()),
                            )?;
                        }

                        self.last_style = cell.style;
                    }

                    queue!(
                        &mut self.output_buf,
                        cursor::MoveTo(x, y),
                        style::Print(cell.ch)
                    )?;
                }

                // Update the previous buffer

                let prev_cell = self.prev_grid.get_mut(x, y);
                // NOTE(Chris): As long as Cell doesn't do any heap allocations, using clone_from()
                // should allow us to avoid making new heap allocations.
                prev_cell.clone_from(cell);
            }
        }

        if self.should_show_cursor {
            let move_to_cmd = cursor::MoveTo(self.cursor_display.0, self.cursor_display.1);

            queue!(&mut self.output_buf, move_to_cmd, cursor::Show,)?;
        } else {
            queue!(&mut self.output_buf, cursor::Hide,)?;
        }

        stdout_lock.write_all(&self.output_buf)?;
        self.output_buf.clear();

        stdout_lock.flush()?;

        Ok(())
    }
}

/// Grid implements a two-dimensional array with a single contiguous buffer
#[derive(Clone)]
pub struct Grid<T> {
    width: u16,
    height: u16,
    buffer: Vec<T>,
}

impl<T> Grid<T>
where
    T: Default,
    T: Clone,
{
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            buffer: vec![T::default(); (width * height).into()],
        }
    }

    fn resize_blunt(&mut self, width: u16, height: u16, value: T) {
        self.width = width;
        self.height = height;
        self.buffer.resize((width * height).into(), value);
    }
}

impl<T> Grid<T> {
    fn get(&self, x: u16, y: u16) -> &T {
        &self.buffer[coords_to_index(self.width, x, y)]
    }

    // This attribute and profile will disable compilation of this function unless testing,
    // eliminating the dead code error for this function.
    #[cfg(test)]
    fn set(&mut self, x: u16, y: u16, value: T) {
        self.buffer[coords_to_index(self.width, x, y)] = value;
    }

    fn get_mut(&mut self, x: u16, y: u16) -> &mut T {
        if x > self.width - 1 {
            panic!("Width is too large: {}", self.width);
        }

        if y > self.height - 1 {
            panic!("Height is too large: {}", self.height);
        }

        &mut self.buffer[coords_to_index(self.width, x, y)]
    }
}

fn coords_to_index(width: u16, x: u16, y: u16) -> usize {
    (y * width + x).into()
}

#[derive(Default)]
pub struct LineBuilder {
    cells: Vec<Cell>,
    last_style: Style,
}

impl LineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, ch: char, style: Style) -> &mut Self {
        self.last_style = style;
        self.cells.push(Cell {
            ch,
            style,
            is_dead: false,
        });
        self
    }

    pub fn push_def(&mut self, ch: char) -> &mut Self {
        self.cells.push(Cell {
            ch,
            style: self.last_style,
            is_dead: false,
        });
        self
    }

    pub fn push_str(&mut self, string: &str) -> &mut Self {
        for ch in string.chars() {
            self.cells.push(Cell {
                ch,
                style: self.last_style,
                is_dead: false,
            });
        }
        self
    }

    pub fn use_style(&mut self, style: Style) -> &mut Self {
        self.last_style = style;
        self
    }

    pub fn use_fg_color(&mut self, fg_color: Color) -> &mut Self {
        self.last_style.fg = fg_color;
        self
    }

    pub fn use_bg_color(&mut self, bg_color: Color) -> &mut Self {
        self.last_style.bg = bg_color;
        self
    }

    pub fn use_attribute(&mut self, attribute: Attribute) -> &mut Self {
        self.last_style.attribute = attribute;
        self
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Cell {
    ch: char,
    style: Style,
    // A dead cell won't be updated until it's made alive
    is_dead: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Style {
    pub attribute: Attribute,
    pub fg: Color,
    pub bg: Color,
}

impl Style {
    pub fn new(attribute: Attribute, fg: Color, bg: Color) -> Self {
        Self { attribute, fg, bg }
    }

    pub fn new_attr(attribute: Attribute) -> Self {
        Self {
            attribute,
            ..Default::default()
        }
    }

    pub fn new_color(fg: Color, bg: Color) -> Self {
        Self {
            fg,
            bg,
            ..Default::default()
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            attribute: Attribute::default(),
            fg: Color::Foreground,
            bg: Color::Background,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground, // Default foreground color
    Background, // Default background color
}

impl Color {
    fn to_crossterm(self) -> style::Color {
        match self {
            Self::Black => style::Color::Black,
            Self::Red => style::Color::DarkRed,
            Self::Green => style::Color::DarkGreen,
            Self::Yellow => style::Color::DarkYellow,
            Self::Blue => style::Color::DarkBlue,
            Self::Magenta => style::Color::DarkMagenta,
            Self::Cyan => style::Color::DarkCyan,
            Self::White => style::Color::DarkGrey,
            Self::BrightBlack => style::Color::Grey,
            Self::BrightRed => style::Color::Red,
            Self::BrightGreen => style::Color::Green,
            Self::BrightYellow => style::Color::Yellow,
            Self::BrightBlue => style::Color::Blue,
            Self::BrightMagenta => style::Color::Magenta,
            Self::BrightCyan => style::Color::Cyan,
            Self::BrightWhite => style::Color::White,
            Self::Foreground => unreachable!("Foreground not convertible to a crossterm color!"),
            Self::Background => unreachable!("Background not convertible to a crossterm color!"),
        }
    }
}

// https://docs.rs/crossterm/0.20.0/crossterm/style/enum.Attribute.html#platform-specific-notes
// Based on the attributes available on both Windows and Unix in crossterm
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Attribute(u8);

#[allow(non_upper_case_globals, dead_code)]
impl Attribute {
    pub const None: Attribute = Attribute(0b00000000);
    pub const Bold: Attribute = Attribute(0b00000001);
    pub const Dim: Attribute = Attribute(0b00000010);
    pub const Underlined: Attribute = Attribute(0b00000100);
    pub const Reverse: Attribute = Attribute(0b00001000);
    pub const Hidden: Attribute = Attribute(0b00010000);

    fn queue_crossterm<T>(self, output: &mut T) -> io::Result<()>
    where
        T: Write,
    {
        if self.contains(Self::Bold) {
            queue!(output, style::SetAttribute(style::Attribute::Bold))?;
        }

        if self.contains(Self::Dim) {
            queue!(output, style::SetAttribute(style::Attribute::Dim))?;
        }
        if self.contains(Self::Underlined) {
            queue!(output, style::SetAttribute(style::Attribute::Underlined))?;
        }
        if self.contains(Self::Reverse) {
            queue!(output, style::SetAttribute(style::Attribute::Reverse))?;
        }
        if self.contains(Self::Hidden) {
            queue!(output, style::SetAttribute(style::Attribute::Hidden))?;
        }

        Ok(())
    }
}

impl BitOr for Attribute {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Attribute {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}

impl BitAnd for Attribute {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl Attribute {
    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_get() {
        let mut grid = Grid::new(4, 5);

        grid.set(2, 3, 'a');
        grid.set(3, 4, 'Z');

        assert_eq!(grid.get(2, 3), &'a');
    }

    #[test]
    fn test_attribute_contains() {
        let attr1 = Attribute::Bold | Attribute::Underlined;
        let attr2 = Attribute::Bold;

        assert!(attr1.contains(attr2));

        let attr3 = Attribute::Dim;

        assert!(!attr1.contains(attr3));
    }
}
