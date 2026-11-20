# Eucalypt Calculator

A keyboard-driven, full-screen Rust calculator for Linux and Windows, inspired by SpeedCrunch and Ziggity.

With Rust 1.88 or later, run `cargo run --locked`, or install with `cargo install --locked --path .` and launch `eucalypt`.

Type an expression for a live preview and press **Enter** to record it. **Up/Down** recall past expressions, **PgUp/PgDn** scroll the calculations, **Esc** clears the prompt, and **Ctrl+C** quits (**Ctrl+Q** also works). Press **?** for shortcut help; **?** or **Esc** closes it without changing your expression. History and variables last for the current session.

Try `2 + 3 * (4 - 1)^2`, `17 % 5`, `sqrt(81)`, or `sin(pi/2)`. Trig starts in radians. Press **Tab** to toggle degrees/radians: in degrees, `sin(90)` gives `1` and `asin(1)` gives `90`. The expression title shows the active unit.

`%` gives a nonnegative remainder. Assign a variable with `x = 54`, then use `x * 2` or `ans + 1`. Results are rounded for readability and use commas and a decimal point (e.g. `132,312.23123`); numbers can be entered with or without the same grouping commas. Calculations use double-precision floating point, with very large or small results shown in scientific notation.
