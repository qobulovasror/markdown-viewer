# Demo recording for the README. Generate with:  vhs assets/demo.tape
# Requires: vhs (https://github.com/charmbracelet/vhs) and a release build (cargo build --release).
# The tape puts target/release first on PATH and ignores your config and state.

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
Type "rm -rf /tmp/mdvw-demo; export PATH=$PWD/target/release:$PATH XDG_CONFIG_HOME=/tmp/mdvw-demo XDG_STATE_HOME=/tmp/mdvw-demo && cd examples && mdvw --version > /dev/null && clear"
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

# Table of contents, filtered with /
Type "t"
Sleep 1s
Type "/dia"
Sleep 900ms
Enter
Sleep 300ms
Enter
Sleep 1.5s

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

# Search every file in the folder, open a hit
Type "S"
Sleep 400ms
Type "sideways"
Enter
Sleep 1.2s
Enter
Sleep 1.2s

# Long code lines: stop wrapping, scroll sideways
Type "W"
Sleep 900ms
Right
Sleep 400ms
Right
Sleep 400ms
Right
Sleep 900ms
Type "W"
Sleep 300ms

# Sequence diagram with loop/alt frames
Type "]"
Sleep 1.8s

# Themes
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
