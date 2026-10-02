# Demo recording for the README. Generate with:  vhs assets/demo.tape
# Requires: vhs (https://github.com/charmbracelet/vhs) and mdvw on PATH (cargo install --path .)

Output assets/demo.gif

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 720
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

Hide
# Warm up: the first launch of a new binary can be slow on macOS.
Type "cd examples && mdvw --version > /dev/null && clear"
Enter
Sleep 2s
Show

Type "mdvw sample.md"
Sleep 300ms
Enter
Sleep 1.5s

# Scroll and jump between headings
Type "jjjjj"
Sleep 500ms
Type "]"
Sleep 800ms
Type "]"
Sleep 800ms

# Table of contents
Type "t"
Sleep 1s
Type "jj"
Sleep 500ms
Enter
Sleep 1.2s

# Search
Type "/table"
Sleep 800ms
Enter
Sleep 1s
Escape
Sleep 300ms

# Links: focus the footnote reference, follow it, then go back
Type "g"
Tab
Sleep 500ms
Tab
Sleep 600ms
Enter
Sleep 1s
Backspace
Sleep 800ms

# Themes
Type "T"
Sleep 900ms
Type "T"
Sleep 900ms
Type "T"
Sleep 900ms

# Fuzzy finder
Type "o"
Sleep 600ms
Type "org"
Sleep 800ms
Enter
Sleep 1.5s

# Help
Type "?"
Sleep 1.5s
Escape
Type "q"
Sleep 500ms
