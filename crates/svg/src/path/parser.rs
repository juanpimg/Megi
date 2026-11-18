use super::SubPath;
use super::flatten::arc_to_cubics;
use super::{flatten_cubic, flatten_quadratic};
use crate::{Limits, SvgError};

/// Parses an SVG path `d` string into flattened sub-paths.
///
/// `segments_used` is the document-wide counter (see
/// [`Limits::max_path_segments`](crate::Limits::max_path_segments)); it is charged as
/// points accumulate, not only when a sub-path is flushed.
pub(crate) fn parse_path_data(
    raw: &str,
    limits: &Limits,
    used: &mut usize,
) -> Result<Vec<SubPath>, SvgError> {
    let mut scanner = Scanner {
        text: raw,
        bytes: raw.as_bytes(),
        position: 0,
    };
    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let mut current_point = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);
    let mut last_cubic_control: Option<(f32, f32)> = None;
    let mut last_quadratic_control: Option<(f32, f32)> = None;
    let mut last_command: Option<char> = None;
    let mut started = false;

    let count = |current: &Vec<(f32, f32)>, used: &mut usize| -> Result<(), SvgError> {
        *used = used.saturating_add(current.len());
        if *used > limits.max_path_segments {
            return Err(SvgError::Limit(format!(
                "document exceeds {} flattened segments",
                limits.max_path_segments
            )));
        }
        Ok(())
    };

    loop {
        scanner.skip_separators();
        if scanner.at_end() {
            break;
        }
        let command = scanner.peek_char();
        let command = if command.is_ascii_alphabetic() {
            scanner.position += 1;
            command
        } else if let Some(previous) = last_command {
            // Implicit repetition: `M`/`m` become line commands, others repeat.
            match previous {
                'M' => 'L',
                'm' => 'l',
                other => other,
            }
        } else {
            return Err(SvgError::Invalid(
                "path data must start with a command".to_owned(),
            ));
        };

        match command {
            'M' | 'm' => {
                if started && !current.is_empty() {
                    count(&current, used)?;
                    subpaths.push(SubPath {
                        points: std::mem::take(&mut current),
                        closed: false,
                    });
                }
                let (x, y) = scanner.next_point()?;
                let target = if command == 'm' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                current_point = target;
                subpath_start = target;
                current.push(target);
                started = true;
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'L' | 'l' => {
                let (x, y) = scanner.next_point()?;
                let target = if command == 'l' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'H' | 'h' => {
                let x = scanner.next_number()?;
                let target = if command == 'h' {
                    (current_point.0 + x, current_point.1)
                } else {
                    (x, current_point.1)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'V' | 'v' => {
                let y = scanner.next_number()?;
                let target = if command == 'v' {
                    (current_point.0, current_point.1 + y)
                } else {
                    (current_point.0, y)
                };
                current_point = target;
                current.push(target);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'C' | 'c' => {
                let (x1, y1) = scanner.next_point()?;
                let (x2, y2) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x1, y1, x2, y2, x, y) = if command == 'c' {
                    (
                        current_point.0 + x1,
                        current_point.1 + y1,
                        current_point.0 + x2,
                        current_point.1 + y2,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x1, y1, x2, y2, x, y)
                };
                flatten_cubic(current_point, (x1, y1), (x2, y2), (x, y), &mut current);
                last_cubic_control = Some((x2, y2));
                last_quadratic_control = None;
                current_point = (x, y);
            }
            'S' | 's' => {
                let (x2, y2) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x2, y2, x, y) = if command == 's' {
                    (
                        current_point.0 + x2,
                        current_point.1 + y2,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x2, y2, x, y)
                };
                let control1 = match last_command {
                    Some('C' | 'c' | 'S' | 's') => last_cubic_control.map_or(current_point, |c| {
                        (2.0 * current_point.0 - c.0, 2.0 * current_point.1 - c.1)
                    }),
                    _ => current_point,
                };
                flatten_cubic(current_point, control1, (x2, y2), (x, y), &mut current);
                last_cubic_control = Some((x2, y2));
                last_quadratic_control = None;
                current_point = (x, y);
            }
            'Q' | 'q' => {
                let (x1, y1) = scanner.next_point()?;
                let (x, y) = scanner.next_point()?;
                let (x1, y1, x, y) = if command == 'q' {
                    (
                        current_point.0 + x1,
                        current_point.1 + y1,
                        current_point.0 + x,
                        current_point.1 + y,
                    )
                } else {
                    (x1, y1, x, y)
                };
                flatten_quadratic(current_point, (x1, y1), (x, y), &mut current);
                last_quadratic_control = Some((x1, y1));
                last_cubic_control = None;
                current_point = (x, y);
            }
            'T' | 't' => {
                let (x, y) = scanner.next_point()?;
                let (x, y) = if command == 't' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                let control = match last_command {
                    Some('Q' | 'q' | 'T' | 't') => last_quadratic_control
                        .map_or(current_point, |c| {
                            (2.0 * current_point.0 - c.0, 2.0 * current_point.1 - c.1)
                        }),
                    _ => current_point,
                };
                flatten_quadratic(current_point, control, (x, y), &mut current);
                last_quadratic_control = Some(control);
                last_cubic_control = None;
                current_point = (x, y);
            }
            'A' | 'a' => {
                let rx = scanner.next_number()?;
                let ry = scanner.next_number()?;
                let rotation = scanner.next_number()?;
                let large_arc = scanner.next_flag()?;
                let sweep = scanner.next_flag()?;
                let (x, y) = scanner.next_point()?;
                let (x, y) = if command == 'a' {
                    (current_point.0 + x, current_point.1 + y)
                } else {
                    (x, y)
                };
                arc_to_cubics(
                    current_point,
                    (x, y),
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    &mut current,
                );
                current_point = (x, y);
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            'Z' | 'z' => {
                if !current.is_empty() {
                    current.push(subpath_start);
                    count(&current, used)?;
                    subpaths.push(SubPath {
                        points: std::mem::take(&mut current),
                        closed: true,
                    });
                }
                current_point = subpath_start;
                last_cubic_control = None;
                last_quadratic_control = None;
            }
            other => {
                return Err(SvgError::Invalid(format!("unknown path command {other:?}")));
            }
        }
        // Enforce the budget as points accumulate: checking only when a sub-path is
        // flushed lets one `d` attribute grow unbounded (a single cubic can flatten to
        // 4,097 points, so a few hundred curves would allocate gigabytes before the check).
        if used.saturating_add(current.len()) > limits.max_path_segments {
            return Err(SvgError::Limit(format!(
                "document exceeds {} flattened segments",
                limits.max_path_segments
            )));
        }
        // `Z` consumes no input; letting it be implicitly repeated turns `d="M0 0Z1"`
        // into an infinite loop (the scanner position never advances).
        last_command = match command {
            'Z' | 'z' => None,
            other => Some(other),
        };
    }

    if !current.is_empty() {
        count(&current, used)?;
        subpaths.push(SubPath {
            points: current,
            closed: false,
        });
    }
    Ok(subpaths)
}

/// Byte scanner for path data.
struct Scanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Scanner<'a> {
    fn at_end(&self) -> bool {
        self.position >= self.bytes.len()
    }

    fn peek_char(&self) -> char {
        self.bytes
            .get(self.position)
            .copied()
            .unwrap_or(b' ')
            .into()
    }

    fn skip_separators(&mut self) {
        while let Some(byte) = self.bytes.get(self.position) {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b',') {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn next_number(&mut self) -> Result<f32, SvgError> {
        self.skip_separators();
        let rest = self
            .text
            .get(self.position..)
            .ok_or_else(|| SvgError::Invalid("path data is not at a UTF-8 boundary".to_owned()))?;
        match crate::transform::parse_one_number(rest) {
            Some((number, consumed)) if number.is_finite() => {
                self.position += consumed;
                Ok(number)
            }
            _ => Err(SvgError::Invalid("expected a path number".to_owned())),
        }
    }

    fn next_point(&mut self) -> Result<(f32, f32), SvgError> {
        let x = self.next_number()?;
        let y = self.next_number()?;
        Ok((x, y))
    }

    fn next_flag(&mut self) -> Result<bool, SvgError> {
        self.skip_separators();
        match self.bytes.get(self.position) {
            Some(b'0') => {
                self.position += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(SvgError::Invalid(
                "expected an arc flag (0 or 1)".to_owned(),
            )),
        }
    }
}
