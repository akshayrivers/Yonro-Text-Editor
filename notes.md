> This document tracks architectural decisions and implementation details while
> building a terminal text editor from scratch in Rust specifically for my personal use as a writer and a programmer.

# Yonro's Text Editor - Implementation Notes & Learning Journey

Initially it had been a faithful implementation of the text editor built in the [Hecto tutorial](https://www.flenker.blog/hecto/). But I have enhanced and changed it into something which I can use as a writer and also it is also to challenge myself as a programmer.

## Table of Contents

### Core Development Phases

1. [Phase I: Raw I/O Mode & Keypressing](#phase-i-raw-io-mode--keypressing)
2. [Phase II: Text Viewing & Caret Movement](#phase-ii-text-viewing--caret-movement)
3. [Phase III: Text Editing](#phase-iii-text-editing)
4. [Phase IV: Search & Matching](#phase-iv-search--matching)
5. [Phase V: Syntax Highlighting](#phase-v-syntax-highlighting)

### Editor Features & Extensions

6. [Phase VI: Miscellaneous Features](#phase-vi-miscellaneous-features)
   - 6.1 Undo/Redo
   - 6.2 Copy/Paste
   - 6.3 Unit tests and Logging
7. [Phase VII: Plugin System](#phase-vii-plugin-system)

### Future Work

8. [Future Plans](#future-plans)
9. [Bugs/Backlog](#bugs--backlog)

### Narrative Studio (GUI)

10. [GUI Narrative Studio](#gui-narrative-studio-tauri)

---

## Phase I: Raw I/O Mode & Keypressing

### Terminal Mode Abstraction

Different operating systems handle terminal I/O differently:

- **Unix/Linux**: POSIX termios API for raw mode
- **Windows**: Console API with different flags and modes
- **macOS**: Inherited from Unix but with specific quirks

**Solution**: Use the `crossterm` crate for cross-platform abstraction.

### What is Raw Mode?

Raw mode disables input/output processing, allowing real-time keystroke detection without waiting for Enter:

```
Normal Mode:        Raw Mode:
User types: a       Immediate event: KeyPress('a')
User types: b       Immediate event: KeyPress('b')
User types: ↵       Immediate event: KeyPress('Enter')
Process 'ab↵'       Each keystroke processed individually
```

### Crossterm: Cross-Platform Terminal Manipulation

**Why Crossterm?**

- Pure Rust library (no C dependencies)
- Supports UNIX and Windows down to Windows 7
- Better performance than alternatives
- Comprehensive API with command system

#### Key Modules

| Module      | Purpose                                    |
| ----------- | ------------------------------------------ |
| `terminal`  | Screen size, raw mode, alternate screen    |
| `cursor`    | Position, show/hide, shape                 |
| `event`     | Keyboard/mouse input                       |
| `style`     | Colors, attributes (bold, underline, etc.) |
| `clipboard` | System clipboard access                    |

### Command Execution Patterns

Crossterm offers two execution strategies:

#### 1. **Lazy Execution (Queuing)**

```rust
use crossterm::{queue, ExecutableCommand};
use std::io::{stdout, Write};

let mut stdout = stdout();
queue!(stdout, MoveTo(0, 0), Print("Hello"))?;
stdout.flush()?;  // Actually execute
```

**Advantages:**

- Better performance (fewer system calls)
- Batch operations
- Full control over flush timing

**Use case:** Real-time TUI rendering (text editors, dashboards)

#### 2. **Direct Execution**

```rust
use crossterm::execute;

execute!(stdout(), MoveTo(0, 0), Print("Hello"))?;
```

**Advantages:**

- Simpler API
- Immediate execution

**Use case:** One-off operations, scripts

**In our code:** We extensively use lazy execution with `queue!` because text editors need optimal performance.

### Implementation: Raw Mode Setup

```rust
use crossterm::{
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use std::io::stdout;

pub fn enable_editor_mode() -> Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Ok(())
}

pub fn disable_editor_mode() -> Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
```

**What happens:**

1. `enable_raw_mode()` - Terminal stops processing, sends keystrokes directly
2. `EnterAlternateScreen` - Switches to alternate buffer (preserves terminal state)
3. On exit: Reverse operations restore terminal

### Handling Input Events

```rust
use crossterm::event::{self, Event, KeyCode, KeyModifiers};

match event::read()? {
    Event::Key(key_event) => {
        match key_event.code {
            KeyCode::Char('q') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Q pressed
            }
            KeyCode::Char(c) => {
                // Regular character
            }
            KeyCode::Enter => {
                // Newline
            }
            _ => {}
        }
    }
    Event::Resize(width, height) => {
        // Terminal resized
    }
    _ => {}
}
```

**Key Learning:** Non-blocking input polling is crucial for responsive editors.

---

## Phase II: Text Viewing & Caret Movement

### Data Structure: Line-based Storage

```rust
pub struct Buffer {
    lines: Vec<Line>,
    file_info: FileInfo,
    dirty: bool,
}

pub struct Line {
    fragments: Vec<TextFragment>,
    string: String,
}
pub struct TextFragment {
    pub grapheme: String,
    pub rendered_width: GraphemeWidth,
    pub replacement: Option<char>,
    pub start_byte_idx: ByteIdx,
}
```

**Why Vec of Lines?**

- Easy horizontal navigation within lines
- Fast vertical navigation (random access)
- Natural file structure representation
- Efficient rendering (render only visible lines)

### Scrolling & Viewport Management

The key challenge: **Map cursor position to screen position**

```
File content:         Screen (24 lines):
Line 1    ┐           Line 50 (visible as line 1)
Line 2    │ offset    Line 51 (visible as line 2)
Line 3    │ = 49      Line 52 (visible as line 3)
...       │           ...
Line 50   ├─ snap     Line 73 (visible as line 24)
Line 51 * │           * = cursor position
Line 52   │
Line 53   │
...       ┘
```

so we went with visualizing a canvas of infinite rows and columns from which we cna choose the size of the terminal to show them correctly
Ultimately we have used various snapping and centering functions to keep the content correct.

### Grapheme Support: Unicode Complexity

#### The Problem

```rust
// These look like 3 characters but aren't:
"e" + "◌́" = "é"        // e + combining acute
"👨" + "👩" + "👧" = "👨‍👩‍👧"  // Family emoji (ZWJ sequence)

// String indexing is dangerous:
"hello".chars().nth(2)        // Safe: 'l'
"café".chars().nth(3)         // Unsafe: accented é
"👨‍👩‍👧".chars().nth(1)  // Wrong: gets middle component
```

#### Solution: Grapheme Segmentation

```rust
use unicode_segmentation::UnicodeSegmentation;

let text = "café";
let graphemes: Vec<&str> = text.graphemes(true).collect();
// graphemes = ["c", "a", "f", "é"]  ✓ Correct!

let text = "👨‍👩‍👧";
let graphemes: Vec<&str> = text.graphemes(true).collect();
// graphemes = ["👨‍👩‍👧"]  ✓ Entire emoji family as one unit!


    // Ensures self.location.grapheme_idx points to a valid grapheme index by snapping it to the left most grapheme if appropriate.
    // Doesn't trigger scrolling.
    fn snap_to_valid_grapheme(&mut self) {
        self.text_location.grapheme_idx = min(
            self.text_location.grapheme_idx,
            self.buffer.grapheme_count(self.text_location.line_idx),
        )
    }
    // Ensures self.location.line_idx points to a valid line index by snapping it to the bottom most line if appropriate.
    // Doesn't trigger scrolling.
    fn snap_to_valid_line(&mut self) {
        self.text_location.line_idx = min(self.text_location.line_idx, self.buffer.height());
    }
```

#### Implementation in Editor

```rust
pub struct Line {
    fragments: Vec<TextFragment>,
    string: String,
}

impl Line {
...
}
impl Display for Line {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.string)
    }
}

impl Deref for Line {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.string
    }
}
pub struct TextFragment {
    pub grapheme: String,
    pub rendered_width: GraphemeWidth,
    pub replacement: Option<char>,
    pub start_byte_idx: ByteIdx,
}

```

#### Width Calculation

Graphemes have varying display widths:

```rust
use unicode_width::UnicodeWidth;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

"a".width()           // 1 (normal ASCII)
"é".width()           // 1 (combining character)
"👨".width()          // 2 (emoji - double width)
"➜".width()           // 2 (symbol - double width)
"\t".width()          // 8 (tab - variable)


#[derive(Copy, Clone, Debug)]
pub enum GraphemeWidth {
    Half,
    Full,
}
impl From<GraphemeWidth> for usize {
    fn from(val: GraphemeWidth) -> Self {
        match val {
            GraphemeWidth::Half => 1,
            GraphemeWidth::Full => 2,
        }
    }
}

```

This is critical for:

- Cursor positioning
- Line wrapping
- Text alignment

**Key Learning:** Never count characters when positioning; always use `UnicodeWidth`.

---

## Phase III: Text Editing

Under the hood we have primarily used the crossterms events to execute the commands which we are differentiating on our own custom logic

```rust
pub enum Command {
    Move(Move),
    Edit(Edit),
    System(System),
}
pub enum Move {
    PageUp,
    PageDown,
    StartOfLine,
    EndOfLine,
    Up,
    Left,
    Right,
    Down,
}
pub enum Edit {
    Insert(char),
    InsertNewLine,
    Delete,
    DeleteBackward,
}
pub enum System {
    Save,
    Resize(Size),
    Quit,
    Dismiss,
    Search,
}

```

Where we first correctly track which command is issued and which underlying function needs to performed
and the commands are then dispatched though process_command:

```rust
    fn process_command(&mut self, command: Command) {
        if let System(Resize(size)) = command {
            self.handle_resize_command(size);
            return;
        }
        match self.prompt_type {
            PromptType::Search => self.process_command_during_search(command),
            PromptType::Save => self.process_command_during_save(command),
            PromptType::None => self.process_command_no_prompt(command),
        }
    }
```

we have tried to divulge work further into more functions to keep the flow clean and modular

```rust
 ...
    fn insert_char(&mut self, character: char) {
        let old_len = self.buffer.grapheme_count(self.text_location.line_idx);
        self.buffer.insert_char(character, self.text_location);
        let new_len = self.buffer.grapheme_count(self.text_location.line_idx);
        let grapheme_delta = new_len.saturating_sub(old_len);
        if grapheme_delta > 0 {
            // we move right with scroll handling
            self.handle_move_command(Move::Right);
        }
        self.mark_redraw(true);
    }
    ...
```

## Phase IV: Search & Fuzzy Matching

### Search Architecture

Since our editor already maintains a **line-based buffer with grapheme-aware indexing**, implementing search becomes a matter of:

1. Tracking search query
2. Traversing buffer line-by-line
3. Returning match locations
4. Highlighting matches during rendering

The search system is built around three core components:

- **Editor Search State**
- **Buffer Search Traversal**
- **Line-Level Matching**

---

### Search State Management

Search state is stored using a `SearchInfo` structure:

- Current query
- Selected match
- Navigation direction

Search begins from the current cursor location:

```rust
pub fn search(&mut self, query: &str) {
    if let Some(search_info) = &mut self.search_info {
        search_info.query = Some(Line::from(query));
    }
    self.search_in_direction(self.text_location, SearchDirection::default());
}
```

#### Key Design Decision

Search is **cursor-relative**, meaning:

- Search starts from cursor
- Navigation continues from last match
- Matches wrap around buffer

This mimics behavior of editors like:

- Vim
- VSCode
- Sublime

---

### Directional Search

Search supports:

- Forward search
- Backward search
- Next match
- Previous match

Core search dispatcher:

```rust
fn search_in_direction(&mut self, from: Location, direction: SearchDirection)
```

Behavior:

- Extract query
- Choose search direction
- Search buffer
- Move cursor to result
- Center viewport

This keeps search responsive and user-friendly.

---

### Next / Previous Match Navigation

Search navigation is implemented using:

```rust
pub fn search_next(&mut self)
pub fn search_prev(&mut self)
```

#### Forward Search

Forward search advances cursor:

```rust
let step_right = min(query.grapheme_count(), 1);
```

This ensures:

- Avoids infinite loops
- Moves at least one grapheme forward

---

### Buffer Search Implementation

The buffer performs **line-by-line traversal**:

```rust
pub fn search_forward(&self, query: &str, from: Location)
```

Key idea:

- Iterate lines
- Search within line
- Return first match

Implementation uses iterator chaining:

```rust
.lines
.iter()
.enumerate()
.cycle()
.skip(from.line_idx)
.take(self.lines.len() + 1)
```

### Why `.cycle()` ?

This enables **wrap-around search**:

```
File:

Line 1
Line 2
Line 3
Line 4

Cursor at Line 3

Search Forward:

Line 3 → Line 4 → Line 1 → Line 2
```

This matches modern editor behavior.

---

### Line-Level Search

Each `Line` handles substring matching:

```rust
pub fn search_forward(
    &self,
    query: &str,
    from_grapheme_idx: GraphemeIdx,
)
```

Steps:

1. Convert grapheme index → byte index
2. Extract substring
3. Run `match_indices()`
4. Convert byte index → grapheme index

This ensures:

- Unicode safety
- Grapheme correctness
- Accurate cursor placement

---

### Finding All Matches

Search uses:

```rust
pub fn find_all(&self, query: &str, range: Range<ByteIdx>)
```

This returns:

```rust
Vec<(ByteIdx, GraphemeIdx)>
```

Why both?

| Value       | Purpose            |
| ----------- | ------------------ |
| ByteIdx     | Rendering          |
| GraphemeIdx | Cursor positioning |

This separation is crucial because:

- Rendering uses byte offsets
- Cursor uses grapheme positions

---

##Search Highlighting

Once matches are found, we highlight them during rendering.

This is implemented using:

```rust
SearchResultHighlighter
```

This component:

- Finds all matches in line
- Marks selected match
- Returns annotations

---

### Highlighting Architecture

Rendering pipeline:

```
Line
 ↓
Syntax Highlighter
 ↓
Search Highlighter
 ↓
Annotations
 ↓
Renderer
```

Search highlighting integrates cleanly with syntax highlighting.

---

### Highlight All Matches

```rust
fn highlight_matched_words(&self, line: &Line, result: &mut Vec<Annotation>)
```

This:

- Finds all matches
- Creates annotations
- Marks match ranges

```rust
Annotation {
    annotation_type: AnnotationType::Match,
    start_byte_idx,
    end_byte_idx
}
```

This keeps rendering stateless and modular.

---

### Highlight Selected Match

The currently selected match is highlighted differently:

```rust
AnnotationType::SelectedMatch
```

This enables:

- Bright highlight for current match
- Dim highlight for other matches

Example:

```
hello world hello

      ^ selected
```

---

### Annotation System

Annotations are used for:

- Syntax highlighting
- Search highlighting
- Future features

Example:

```rust
pub struct Annotation {
    annotation_type: AnnotationType,
    start_byte_idx: ByteIdx,
    end_byte_idx: ByteIdx,
}
```

This design allows:

- Multiple overlapping highlights
- Layered rendering
- Plugin extensibility

---

### Why Highlight During Rendering?

Instead of modifying buffer:

We:

- Keep buffer immutable
- Apply highlights dynamically

Advantages:

- No mutation overhead
- Cleaner architecture
- Plugin-friendly

---

## Current Search Features

Currently implemented:

- Forward search
- Backward search
- Wrap-around search
- Highlight all matches
- Highlight selected match
- Grapheme-safe matching

---

## Future Improvements

## Case-Insensitive Search

Example:

```
Hello
hello
HELLO
```

---

## Regex Search

Possible integration:

- `regex`
- `fancy-regex`

---

## Fuzzy Search

Instead of exact match:

```
query: edt

matches:

editor
edited
editing
```

Possible libraries:

- fuzzy-matcher
- skim

---

## Search Panel

Future UI:

```
Search: hello
Matches: 12
```

---

## Incremental Search

Search updates as user types:

```
h → matches
he → refined matches
hel → final matches
```

---

### Key Learning

Search in text editors is deceptively complex:

- Unicode safe indexing
- Efficient traversal
- Rendering highlights
- Navigation state

Even "simple search" requires:

- Buffer traversal
- Unicode correctness
- Rendering integration

This implementation now provides a **solid foundation** for:

- Fuzzy search
- Regex search
- LSP references
- Symbol search

---

## Phase V: Syntax Highlighting

For building the highlighter, we implemented a trait-based architecture that allows different syntax highlighters to be plugged in depending on the file type. Currently, three file types are supported:

- Rust
- Markdown
- Plain Text

Each syntax highlighter implements the `SyntaxHighlighter` trait and is dynamically dispatched through the main `Highlighter` struct. This design keeps the highlighting logic modular and extensible, making it straightforward to add support for additional languages in the future.

The `Highlighter` combines syntax highlighting and search result highlighting. Both highlighters operate independently and return annotations which are merged during rendering.

Unlike the earlier implementation, highlights are now computed **on demand** rather than being cached. This removes synchronization issues, reduces memory usage, and simplifies invalidation logic.

```rust
#[derive(Default)]
pub struct Highlighter<'a> {
    syntax_highlighter: Option<Box<dyn SyntaxHighlighter>>,
    search_result_highlighter: Option<SearchResultHighlighter<'a>>,
}
```

The `SyntaxHighlighter` trait defines a single method that computes annotations for a given line:

```rust
pub trait SyntaxHighlighter {
    fn highlight(&mut self, idx: LineIdx, line: &Line) -> Vec<Annotation>;
}
```

Depending on the file type, an appropriate syntax highlighter is created:

```rust
fn create_syntax_highlighter(file_type: FileType) -> Option<Box<dyn SyntaxHighlighter>> {
    match file_type {
        FileType::Rust => Some(Box::<RustSyntaxHighlighter>::default()),
        FileType::Text => None,
        FileType::MarkDown => None,
    }
}
```

Search highlighting is implemented using a dedicated highlighter that operates alongside syntax highlighting:

```rust
#[derive(Default)]
pub struct SearchResultHighlighter<'a> {
    matched_word: &'a str,
    selected_match: Option<Location>,
}
```

Both syntax highlighting and search highlighting generate `Annotation` values, which are merged by the main `Highlighter` before rendering.

This architecture provides:

- Modular syntax highlighting
- Dynamic dispatch for extensibility
- Stateless, on-demand highlighting
- Clean separation of concerns
- Easy addition of new languages

---

## Phase VI: Miscellaneous Features

### Undo / Redo

First I went along with the simple model of using 2 stacks one for undo and one for redo and then inserting in commands and therir relevent data.
we pop out of the stack and push into the other stack
|
\/
reverse the command
|
\/
Apply the reversed operation

```rust
    // very simple undo redo
    pub fn redo(&mut self) {
        if let Some(op) = self.redo_stack.pop() {
            self.apply_operation(&op);

            self.undo_stack.push(op);

            self.mark_redraw(true);
        }
    }
    pub fn undo(&mut self) {
        if let Some(op) = self.undo_stack.pop() {
            let rev = Self::reverse(&op);

            self.apply_operation(&rev);

            self.redo_stack.push(op);

            self.mark_redraw(true);
        }
    }
    fn reverse(op: &EditOperation) -> EditOperation {
        match op {
            EditOperation::Insert { at, text } => EditOperation::Delete {
                at: *at,
                text: *text,
            },
            EditOperation::Delete { at, text } => EditOperation::Insert {
                at: *at,
                text: *text,
            },
        }
    }
    fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { at, text } => {
                self.buffer.insert_char(*text, *at);
                self.text_location = Location {
                    line_idx: at.line_idx,
                    grapheme_idx: at.grapheme_idx + 1,
                };
            }
            EditOperation::Delete { at, .. } => {
                self.buffer.delete(*at);
                self.text_location = *at;
            }
        }
    }
```

This model only stores a single char. It had no concept of structues, so the moment one presses Enter or Backspace at a line boundary - we are merging or splitting two lines. The reverse() fxn cannot handle that beacuse there is nothing into. It would in undesired results and may even corrupt the buffer.

So, to handle this correctly I intoduced the functionality to handle InserNewLine and DeleteNewLine

```rust
enum EditOperation {
    // NOTE: text is char (single Unicode scalar value) not a grapheme cluster.
    // This is safe for keyboard input since the OS composes grapheme clusters
    // before delivery. If paste support is added, this must change to String.
    InsertChar {
        at: Location,
        text: char,
    },
    DeleteChar {
        at: Location,
        text: char,
    },
    InsertNewLine {
        at: Location,
        grapheme_count_at_split: usize,
    },
    DeleteNewLine {
        line_idx: usize,
        split_at_grapheme: usize,
    },
    InsertGroup {
        start: Location,
        chars: Vec<char>,
    },
}
```

delete_backward() fxn was also fixed to record the op before moving the cursor so the stored location is always the true deletion point.Also I replaced apply_operation() + reverse() pair with apply_undo + apply_redo because cursor placement logic differs between undo and redo even for the same operation type:

```rust
    // hmm not so very simple undo redo anymore ig
    pub fn undo(&mut self) {
        if let Some(op) = self.undo_stack.pop() {
            self.apply_undo(&op);
            self.redo_stack.push(op);
            self.scroll_text_location_into_view();
            self.mark_redraw(true);
        }
    }

    pub fn redo(&mut self) {
        if let Some(op) = self.redo_stack.pop() {
            self.apply_redo(&op);
            self.undo_stack.push(op);
            self.scroll_text_location_into_view();
            self.mark_redraw(true);
        }
    }
    fn apply_undo(&mut self, op: &EditOperation) {
        match op {
            EditOperation::InsertChar { at, .. } => {
                // Undo an insert = delete the character that was inserted
                self.buffer.delete(*at);
                self.text_location = *at;
            }
            EditOperation::DeleteChar { at, text } => {
                // Undo a delete = re-insert the character
                self.buffer.insert_char(*text, *at);
                self.text_location = *at;
            }
            EditOperation::InsertNewLine {
                at,
                grapheme_count_at_split,
            } => {
                // Undo an Enter = merge the line that was split back together.
                // After insert_newline at `at`, the cursor moved to {line_idx+1, grapheme_idx:0}.
                // The split point is at `at.grapheme_idx` on line `at.line_idx`.
                // To undo: delete the newline = buffer.delete at end of at.line_idx
                self.buffer.delete(Location {
                    line_idx: at.line_idx,
                    grapheme_idx: *grapheme_count_at_split,
                });
                self.text_location = *at;
            }
            EditOperation::DeleteNewLine {
                line_idx,
                split_at_grapheme,
            } => {
                // Undo a newline deletion = re-split the merged line
                self.buffer.insert_newline(Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                });
                self.text_location = Location {
                    line_idx: line_idx.saturating_add(1),
                    grapheme_idx: 0,
                };
            }
            EditOperation::InsertGroup { start, chars } => {
                // Delete all chars in the group, working backwards from the last
                // inserted position so byte indices stay valid.
                // The chars were inserted left-to-right starting at `start`.
                // After inserting N chars, the last char is at start.grapheme_idx + N - 1.
                let end_grapheme = start.grapheme_idx.saturating_add(chars.len());
                for grapheme_idx in (start.grapheme_idx..end_grapheme).rev() {
                    self.buffer.delete(Location {
                        line_idx: start.line_idx,
                        grapheme_idx,
                    });
                }
                self.text_location = *start;
            }
        }
    }
    fn apply_redo(&mut self, op: &EditOperation) {
        match op {
            EditOperation::InsertChar { at, text } => {
                self.buffer.insert_char(*text, *at);
                self.text_location = Location {
                    line_idx: at.line_idx,
                    grapheme_idx: at.grapheme_idx.saturating_add(1),
                };
            }
            EditOperation::DeleteChar { at, .. } => {
                self.buffer.delete(*at);
                self.text_location = *at;
            }
            EditOperation::InsertNewLine { at, .. } => {
                self.buffer.insert_newline(*at);
                self.text_location = Location {
                    line_idx: at.line_idx.saturating_add(1),
                    grapheme_idx: 0,
                };
            }
            EditOperation::DeleteNewLine {
                line_idx,
                split_at_grapheme,
            } => {
                // Redo the merge: delete the newline at end of line_idx
                self.buffer.delete(Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                });
                self.text_location = Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                };
            }
            EditOperation::InsertGroup { start, chars } => {
                // Re-insert chars left-to-right
                let mut current = *start;
                for &ch in chars {
                    self.buffer.insert_char(ch, current);
                    current.grapheme_idx = current.grapheme_idx.saturating_add(1);
                }
                self.text_location = current;
            }
        }
    }
```

Now let me address what this InsertGroup is:
So ultimately in modern editors and text viewers the undo and redo functions also support grouping,like if I type a word very quickly and then I do undo, the whole word is gone. Now it can also be acheived through char by char iteration but, I decided to build support for it through

```rust
enum EditOperation {
    // ... previous variants
    InsertGroup {
        start: Location,  // location of the first char in the group
        chars: Vec<char>, // all chars inserted, in order
    },
}

const GROUP_TIMEOUT_MS: u128 = 800;
// I have kept it 800 beacuse it was the nice spot for me
```

When a new InsertChar arrives, three cases:

- No recent insert or timeout exceeded → push a fresh InsertChar
- Last op is InsertChar and within timeout → upgrade both into an InsertGroup
- Last op is already an InsertGroup and within timeout → append to it

Typing "hello" within 800ms:

h pressed → InsertChar { 'h' }
e pressed → timeout ok, last is InsertChar → upgrade to InsertGroup { "he" }
l pressed → timeout ok, last is InsertGroup → append → InsertGroup { "hel" }
l pressed → InsertGroup { "hell" }
o pressed → InsertGroup { "hello" }

One undo removes all 5 characters at once.
Cursor returns to where 'h' was inserted.
Any non-insert operation (Delete, Backspace, Enter) sets last_insert_time = None, which breaks the current group. Typing after a delete starts a fresh group.
Undoing a group deletes chars right-to-left so byte indices stay valid:

```rust

EditOperation::InsertGroup { start, chars } => {
    let end_grapheme = start.grapheme_idx.saturating_add(chars.len());
    for grapheme_idx in (start.grapheme_idx..end_grapheme).rev() {
    self.buffer.delete(Location {
        line_idx: start.line_idx,
        grapheme_idx,
        });
    }
    self.text_location = \*start;
}
```

Redoing a group re-inserts left-to-right:

```rust
EditOperation::InsertGroup { start, chars } => {
    let mut current = *start;
    for &ch in chars {
        self.buffer.insert_char(ch, current);
        current.grapheme_idx = current.grapheme_idx.saturating_add(1);
    }
    self.text_location = current;
}
```

I have also written extensive unit tests to check this functionality:

- Single insert undo/redo
- Multiple inserts undo char by char
- Newline undo merges lines correctly
- Newline undo in the middle of a line restores both halves
- Backspace at line start restores the newline
- Delete at line end restores the newline
- Cursor lands at the correct location after every operation
- New edit clears the redo stack
- Undo/redo on empty stacks does nothing (no panic)
- Backspace at document start does nothing and pushes nothing to the stack
- Delete at document end does nothing and pushes nothing to the stack
- Unicode characters undo correctly
- Rapid inserts form a single group
- One undo removes the entire group
- Delete between inserts breaks the group into separate undo steps
- Cursor returns to group start position after undoing a group

### Copy / Paste

### Unit Tests &Logging

---

## Phase VII: Plugin System
Need UI for plugin management and register and unresgister at runtime, also supports for scripts for plugins
---

## Future Plans

---

## Bugs / Backlog

---

## GUI Narrative Studio (Tauri)

The terminal editor grew a desktop half: `yonro-gui` (Tauri v2) over the same `.yonro/` workspace the TUI reads (`manuscript.json`, `lore.json`, `scene-<id>.md`; new additive files like `graphs.json`, `lenses.json` are fine, existing shapes never change).

### Adapter seam

Only `crates/yonro-gui/ui/core.js` touches `window.__TAURI__`. Every other UI file calls `core.<fn>()`, so a future pure-web build reimplements one file over WASM. The command list lives in `architecture.md` section 5. No npm, no bundler, no framework: classic `<script>` files in load order, verified by `node --check`.

### Core computes, UI renders

Word counts, `@mention` parsing, graph builds, filters, lenses, timeline, continuity, progress, search, export, and session stats all live in `yonro-core`. JS is allowed UI-only math (scroll positions, caret, layout coordinates, label fuzzy-matching). Consequences:
- Graph filtering (`graph/query.rs`: scope → pov → kinds/text → weight → focus BFS → degree/orphans → cap) and lens engines (`graph/lens.rs`: pair rule, story map, presence matrix) run in Rust with unit tests; the panel only rebuilds the query object.
- Layout (`graph-layout.js`: force sim, bipartite columns, ordered rows) is positions-only and never invents nodes or edges.

### Workspace honesty

Opening or creating a project probes write access first (`mkdir` + throwaway file) and names the path on failure. Relative paths resolve under `$HOME`, not the app's working directory. Switching projects drops every frontend buffer because the backend drops its own — stale ids were once the cause of every "unknown buffer" error. Deleted scene drafts recreate empty on next open instead of failing.

### What I learned

- A single present-but-unlinked entity forms no edges, so the entity graph can be legitimately empty while the presence matrix still sees it. Empty states must name the next action, not just report zero.
- Seeded test data lies unless the generator is checked: a lockstep POV/mention cycle once built a weight-10 hero pair that no real novel would have. Now the seed deals entities into ~8-clusters and walks heroes on a different cycle.
- Debounce the backend, never the truth: per-keystroke work stays local (dirty flag), while stats, mentions, find, and graph re-queries trail on 120–800ms timers.
