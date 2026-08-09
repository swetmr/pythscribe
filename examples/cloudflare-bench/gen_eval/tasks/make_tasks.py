# make_tasks.py — builds tasks.jsonl for the generation-token eval.
#
# Micro tasks are derived from the surfaces covered by
# tests/differential/cpython_corpus.json (580 entries): strings (str_/g_),
# formatting (pf_/fstring_), collections (pc_/dict_/list_/set_/d83_),
# itertools (pi_/pz_), arithmetic/floats (a4_/s3_), decimal (dec_),
# fractions (frac_), control-flow/functions (except_/lambda_/f1_).
#
# Every micro task carries a REFERENCE SOLUTION (kept here, never shipped in
# prompts). expected_stdout is computed by ACTUALLY RUNNING the reference
# under CPython 3.12 at build time — no hand-written expectations. Newlines
# are normalized (\r\n -> \n) and a single trailing newline is stripped;
# comparison at eval time uses the same normalization.
#
# Macro tasks (kind=macro) are self-contained component prompts modeled on
# examples/clones/shared/* (HelloCard, KanbanBoard, YouTubeApp, ...) and are
# verified by compile-success only (plus `pyths expand` for the psc
# condition). Macro tasks run under the ps/psc conditions only: a plain
# Python 3 program has no equivalent for a React component, so the python
# condition is marked not-applicable rather than faked (documented in
# gen_eval/report.md).
#
# Usage:  python make_tasks.py          (writes tasks.jsonl next to itself)
#         python make_tasks.py --check  (re-verify expected_stdout only)

import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "tasks.jsonl"

# --------------------------------------------------------------------------
# Micro tasks: (id, prompt, reference_solution)
# Prompts describe the computation in natural language; they never contain
# the solution code. Each prompt pins the exact print format so stdout is
# comparable byte-for-byte across conditions.
# --------------------------------------------------------------------------

MICRO = [
    # ---- strings (corpus: str_, g_) ----
    ("str_upper_join",
     "Write a program that takes the sentence \"the quick brown fox jumps over the lazy dog\", "
     "uppercases every word, and prints the words joined by \"|\" on a single line.",
     'print("|".join([w.upper() for w in "the quick brown fox jumps over the lazy dog".split()]))'),

    ("str_title_runs",
     "Write a program that title-cases the string \"it's a test and don't stop\" — capitalize the "
     "first letter of every run of alphabetic characters and lowercase the rest of each run "
     "(so a letter right after an apostrophe starts a new run) — and prints the result.",
     "print(\"it's a test and don't stop\".title())"),

    ("str_strip_replace",
     "Write a program that takes the string \"  hello world  \", removes leading and trailing "
     "whitespace, replaces the substring \"world\" with \"pythscribe\", then prints the resulting "
     "string and its length on one line separated by a single space.",
     's = "  hello world  ".strip().replace("world", "pythscribe")\nprint(s, len(s))'),

    ("str_reverse_slice",
     "Write a program that prints two lines: first the string \"compression\" reversed, then every "
     "second character of the original string (positions 0, 2, 4, ...).",
     's = "compression"\nprint(s[::-1])\nprint(s[::2])'),

    ("str_count_find",
     "Write a program that, for the string \"abracadabra\", prints three values on one line "
     "separated by single spaces: the number of occurrences of \"a\", the number of occurrences "
     "of \"bra\", and the index of the first occurrence of \"cad\".",
     's = "abracadabra"\nprint(s.count("a"), s.count("bra"), s.find("cad"))'),

    ("str_prefix_filter",
     "Write a program that, given the list of words [\"undo\", \"redo\", \"unfold\", \"fold\", "
     "\"unlock\"], prints the list of words that start with \"un\" (as a Python-style list of "
     "strings, preserving order).",
     'print([w for w in ["undo", "redo", "unfold", "fold", "unlock"] if w.startswith("un")])'),

    ("str_split_rejoin",
     "Write a program that splits the string \"a,b,,c\" on commas and prints two lines: first the "
     "resulting list (Python-style list of strings, including the empty piece), then the non-empty "
     "pieces joined by \"-\".",
     'parts = "a,b,,c".split(",")\nprint(parts)\nprint("-".join([p for p in parts if p]))'),

    # ---- formatting (corpus: pf_, fstring_) ----
    ("fmt_align",
     "Write a program that prints two lines: first the number 42 right-aligned in a field of "
     "width 10 (space-padded), then the string \"hi\" centered in a field of width 8 padded "
     "with \"*\" characters.",
     'x = 42\nprint(f"{x:>10}")\nprint(f"{\'hi\':*^8}")'),

    ("fmt_float_prec",
     "Write a program that prints the value 3.14159265 rounded to 2 decimal places and to 4 "
     "decimal places, on one line separated by a single space (fixed-point formatting).",
     'v = 3.14159265\nprint(f"{v:.2f} {v:.4f}")'),

    ("fmt_zero_pad",
     "Write a program that prints the numbers 7, 42 and 173, each zero-padded to width 5, one "
     "per line.",
     'for n in [7, 42, 173]:\n    print(f"{n:05d}")'),

    ("fmt_percent",
     "Write a program that prints the value 0.1234 formatted as a percentage with 1 decimal "
     "place (e.g. one-half prints as 50.0%).",
     'print(f"{0.1234:.1%}")'),

    # ---- collections (corpus: pc_, dict_, list_, set_, d83_) ----
    ("counter_most_common",
     "Write a program that counts the letters of \"mississippi\" and prints the 3 most common "
     "letters with their counts, as a Python-style list of (letter, count) tuples in descending "
     "count order (ties broken by first appearance).",
     'from collections import Counter\nprint(Counter("mississippi").most_common(3))'),

    ("counter_repr",
     "Write a program that builds a multiset-style counter from the list [1, 1, 2, 2, 2, 3] "
     "mapping each element to its number of occurrences, and prints it in the form "
     "Counter({...}) with entries ordered by descending count.",
     'from collections import Counter\nprint(Counter([1, 1, 2, 2, 2, 3]))'),

    ("dict_comp_squares",
     "Write a program that builds a dict mapping each integer from 1 to 5 (inclusive) to its "
     "square, and prints the dict.",
     'print({n: n * n for n in range(1, 6)})'),

    ("dict_merge",
     "Write a program that merges the dict {\"a\": 1, \"b\": 2} with the dict {\"b\": 20, \"c\": 30} "
     "(values from the second dict win on key collisions, insertion order preserved: keys of the "
     "first dict first) and prints the merged dict.",
     'print({**{"a": 1, "b": 2}, **{"b": 20, "c": 30}})'),

    ("dict_get_default",
     "Write a program that, for the dict {\"x\": 10}, looks up key \"x\" and key \"y\" using a "
     "lookup that returns the default value -1 when the key is missing, and prints both results "
     "on one line separated by a single space.",
     'd = {"x": 10}\nprint(d.get("x", -1), d.get("y", -1))'),

    ("dict_invert",
     "Write a program that inverts the dict {\"a\": 1, \"b\": 2, \"c\": 3} (values become keys, "
     "keys become values, preserving order) and prints the inverted dict.",
     'print({v: k for k, v in {"a": 1, "b": 2, "c": 3}.items()})'),

    ("list_mutate",
     "Write a program that starts from the list [1, 2], appends 3, extends it with [4, 5], "
     "inserts 0 at the front, then removes and captures the last element. Print the removed "
     "element, then print the final list, on two lines.",
     'xs = [1, 2]\nxs.append(3)\nxs.extend([4, 5])\nxs.insert(0, 0)\nlast = xs.pop()\nprint(last)\nprint(xs)'),

    ("list_slices",
     "Write a program that, for the list of integers 0 through 9, prints three lines: elements "
     "at indices 2 up to (but not including) 7; every third element starting from index 0; and "
     "the first 3 elements of the reversed list.",
     'xs = list(range(10))\nprint(xs[2:7])\nprint(xs[::3])\nprint(xs[::-1][:3])'),

    ("set_ops",
     "Write a program that, for the sets {1, 2, 3, 4, 5} and {4, 5, 6, 7, 8}, prints three "
     "lines: the sorted list of their union, the sorted list of their intersection, and the "
     "sorted list of elements only in the first set.",
     'a = {1, 2, 3, 4, 5}\nb = {4, 5, 6, 7, 8}\nprint(sorted(a | b))\nprint(sorted(a & b))\nprint(sorted(a - b))'),

    # ---- zip / enumerate / itertools (corpus: pz_, pi_) ----
    ("zip_pairs",
     "Write a program that pairs the names [\"ada\", \"grace\", \"alan\"] with the scores "
     "[95, 87, 92] and prints the list of (name, score) tuples.",
     'print(list(zip(["ada", "grace", "alan"], [95, 87, 92])))'),

    ("enumerate_lines",
     "Write a program that prints the items [\"apple\", \"banana\", \"cherry\"] as a numbered "
     "list starting at 1, one per line, in the exact format \"1. apple\".",
     'for i, item in enumerate(["apple", "banana", "cherry"], 1):\n    print(f"{i}. {item}")'),

    ("iter_chain",
     "Write a program that concatenates the iterables [1, 2], [3] and [4, 5] into a single "
     "sequence (without nested loops or manual +) and prints the resulting list.",
     'from itertools import chain\nprint(list(chain([1, 2], [3], [4, 5])))'),

    ("iter_accumulate",
     "Write a program that prints the list of running totals (cumulative sums) of "
     "[3, 1, 4, 1, 5].",
     'from itertools import accumulate\nprint(list(accumulate([3, 1, 4, 1, 5])))'),

    ("iter_combinations",
     "Write a program that prints the list of all 2-element combinations of the characters of "
     "\"abcd\", as tuples in lexicographic order of position.",
     'from itertools import combinations\nprint(list(combinations("abcd", 2)))'),

    ("iter_permutations",
     "Write a program that generates all permutations of [1, 2, 3] and prints two lines: the "
     "total number of permutations, then the list of the first two permutation tuples.",
     'from itertools import permutations\nperms = list(permutations([1, 2, 3]))\nprint(len(perms))\nprint(perms[:2])'),

    # ---- arithmetic / floats (corpus: a4_, s3_) ----
    ("arith_floor_mod",
     "Write a program that prints the four values -17 floor-divided by 5, -17 modulo 5, "
     "17 modulo -5, and 17 floor-divided by -5, on one line separated by single spaces "
     "(Python integer semantics: floor division rounds toward negative infinity and the result "
     "of a modulo takes the sign of the divisor).",
     'print(-17 // 5, -17 % 5, 17 % -5, 17 // -5)'),

    ("arith_pow",
     "Write a program that prints 2 raised to the 20th power and 7 raised to the 7th power, on "
     "one line separated by a single space.",
     'print(2 ** 20, 7 ** 7)'),

    ("float_nan",
     "Write a program that creates a floating-point NaN value and prints whether it equals "
     "itself and whether it differs from itself (two boolean values on one line separated by a "
     "single space, capitalized True/False style).",
     "n = float('nan')\nprint(n == n, n != n)"),

    ("round_banker",
     "Write a program that prints the results of rounding 0.5, 1.5, 2.5 and -0.5 to the nearest "
     "integer using banker's rounding (round-half-to-even), on one line separated by single "
     "spaces.",
     'print(round(0.5), round(1.5), round(2.5), round(-0.5))'),

    ("sum_stats",
     "Write a program that, for the numbers [3, 7, 1, 9, 4], prints the sum, the minimum, the "
     "maximum, and the arithmetic mean formatted to 2 decimal places, all on one line separated "
     "by single spaces.",
     'ns = [3, 7, 1, 9, 4]\nprint(sum(ns), min(ns), max(ns), f"{sum(ns) / len(ns):.2f}")'),

    ("even_square_sum",
     "Write a program that computes the sum of the squares of the even numbers from 1 to 20 "
     "(inclusive) and prints it.",
     'print(sum(n * n for n in range(1, 21) if n % 2 == 0))'),

    # ---- decimal / fractions (corpus: dec_, frac_) ----
    ("decimal_exact",
     "Write a program that uses exact decimal arithmetic (not binary floats) to print two "
     "lines: the exact sum of the decimal numbers written \"0.1\" and \"0.2\", then the exact "
     "sum of \"2.50\" and \"0.25\" (preserving significant decimal places).",
     "from decimal import Decimal\nprint(Decimal('0.1') + Decimal('0.2'))\nprint(Decimal('2.50') + Decimal('0.25'))"),

    ("fraction_arith",
     "Write a program that uses exact rational arithmetic to print two lines: the sum of the "
     "fractions 3/12 and 1/6 in lowest terms (numerator/denominator form), then the fraction "
     "2/4 reduced to lowest terms.",
     "from fractions import Fraction\nprint(Fraction(3, 12) + Fraction(1, 6))\nprint(Fraction(2, 4))"),

    # ---- control flow / functions / classes (corpus: except_, lambda_, f1_) ----
    ("except_custom",
     "Write a program that defines a custom exception type named TooSmallError, a function that "
     "raises it with the message \"got 3\" when its argument is less than 10, calls the function "
     "with 3, catches the exception, and prints \"error: \" followed by the exception message.",
     'class TooSmallError(Exception):\n    pass\n\ndef check(x):\n    if x < 10:\n        raise TooSmallError(f"got {x}")\n    return x\n\ntry:\n    check(3)\nexcept TooSmallError as e:\n    print(f"error: {e}")'),

    ("closure_adder",
     "Write a program with a function make_adder(n) that returns a new function which adds n to "
     "its argument (a closure). Use it to print make_adder(5) applied to 37 and make_adder(-2) "
     "applied to 10, on one line separated by a single space.",
     'def make_adder(n):\n    return lambda x: x + n\nprint(make_adder(5)(37), make_adder(-2)(10))'),

    ("gen_fib",
     "Write a program with a generator function that yields the Fibonacci sequence starting "
     "0, 1, 1, 2, ... and prints the list of the first 10 values.",
     'from itertools import islice\n\ndef fib():\n    a, b = 0, 1\n    while True:\n        yield a\n        a, b = b, a + b\n\nprint(list(islice(fib(), 10)))'),

    ("class_inherit",
     "Write a program with a class Animal whose constructor stores a name and a sound and which "
     "has a speak() method returning \"<name> says <sound>\"; and a subclass Dog whose "
     "constructor takes only a name and always uses the sound \"woof\". Create Animal(\"Cat\", "
     "\"meow\") and Dog(\"Rex\") and print the result of speak() for each, on two lines.",
     'class Animal:\n    def __init__(self, name, sound):\n        self.name = name\n        self.sound = sound\n\n    def speak(self):\n        return f"{self.name} says {self.sound}"\n\nclass Dog(Animal):\n    def __init__(self, name):\n        self.name = name\n        self.sound = "woof"\n\nprint(Animal("Cat", "meow").speak())\nprint(Dog("Rex").speak())'),

    ("match_classify",
     "Write a program with a function that classifies a value using structural pattern "
     "matching: the integer 0 maps to \"zero\", a two-element list [x, y] maps to \"pair x,y\" "
     "(with the values substituted), and anything else maps to \"other\". Print the "
     "classifications of 0, [1, 2] and 7 on one line separated by single spaces.",
     'def classify(v):\n    match v:\n        case 0:\n            return "zero"\n        case [x, y]:\n            return f"pair {x},{y}"\n        case _:\n            return "other"\n\nprint(classify(0), classify([1, 2]), classify(7))'),

    ("sort_by_len",
     "Write a program that sorts the words [\"fig\", \"banana\", \"kiwi\", \"apple\"] by "
     "increasing length and prints the sorted list.",
     'print(sorted(["fig", "banana", "kiwi", "apple"], key=lambda w: len(w)))'),
]

# --------------------------------------------------------------------------
# Macro tasks: self-contained component briefs modeled on
# examples/clones/shared/*. check=compile; conditions ps/psc only.
# --------------------------------------------------------------------------

MACRO_RULES = (
    " Write a single self-contained file: one React-style function component (plus any small "
    "helper functions), all fixture data defined inline in the same file. Do not import CSS or "
    "any local files; style via class names only. Use component state hooks for interactivity."
)

MACRO = [
    ("macro_hello_card",
     "Build a component HelloCard(title, subtitle=None) that renders a card: a heading with the "
     "title, an optional paragraph with the subtitle only when one is given, and a like button "
     "that toggles between an unliked label ('Like') and a liked label ('Liked') when clicked, "
     "tracked in state." + MACRO_RULES),

    ("macro_counter_panel",
     "Build a component CounterPanel() with a number in state (start 0), three buttons "
     "(increment, decrement, reset to 0), and a message that shows 'even' or 'odd' for the "
     "current value. Disable the decrement button when the value is 0." + MACRO_RULES),

    ("macro_todo_list",
     "Build a component TodoApp() managing a todo list in state: a text input plus Add button "
     "appends a new todo (ignore empty input and clear the input after adding), clicking a todo "
     "toggles its done flag, and a footer shows how many todos are not yet done." + MACRO_RULES),

    ("macro_kanban_lite",
     "Build a component KanbanLite() showing three columns (Todo, Doing, Done), each with a "
     "list of card titles held in one state structure. Each card has left/right buttons that "
     "move it to the adjacent column (hide the impossible direction at the edges), and each "
     "column has an input plus Add button to append a new card to that column." + MACRO_RULES),

    ("macro_video_grid",
     "Build a component VideoGrid() modeled on a YouTube-style home page: a search input that "
     "filters an inline list of at least 8 videos (title, channel, views, category) by "
     "case-insensitive title match, a row of category chips that further filter (an 'All' chip "
     "clears the category filter, the active chip is visually marked via a class), and a grid "
     "of video cards showing title, channel and views. Show an empty-state message when nothing "
     "matches." + MACRO_RULES),

    ("macro_tweet_composer",
     "Build a component TweetFeed() modeled on a Twitter-style feed: a compose textarea with a "
     "280-character limit and a live remaining-character counter (the post button is disabled "
     "when empty or over the limit), posting prepends the new tweet to an inline seed list in "
     "state, and each tweet shows author, text and a like button with a per-tweet like count "
     "that increments on click." + MACRO_RULES),

    ("macro_playlist_player",
     "Build a component PlaylistPlayer() modeled on a Spotify-style view: a sidebar listing at "
     "least 3 playlists (name + track count) where clicking selects the active playlist, a main "
     "panel listing the active playlist's tracks (title, artist, duration) from inline data, "
     "clicking a track makes it the 'now playing' track (highlighted via a class), and a bottom "
     "bar showing the now-playing title with a play/pause toggle button held in state."
     + MACRO_RULES),

    ("macro_course_cards",
     "Build a component CourseCatalog() modeled on a Coursera-style catalog: tabs 'All', "
     "'Enrolled' and 'Completed' filter an inline list of at least 6 courses (title, provider, "
     "progress percent 0-100). Each card shows title, provider, a progress bar element whose "
     "width style is the progress percent, and an Enroll/Unenroll toggle button that flips the "
     "course's enrolled flag in state. A course counts as completed when progress is 100."
     + MACRO_RULES),

    ("macro_movie_rows",
     "Build a component MovieBrowser() modeled on a Netflix-style browse page: a hero section "
     "showing a featured title and description, then two horizontal rows ('Trending', 'New') of "
     "movie cards from inline data (title, year, rating). Clicking a card opens an inline "
     "detail panel (title, year, rating, description) with a Close button; the selected movie "
     "lives in state and only one panel is open at a time." + MACRO_RULES),
]


def run_ref(src: str) -> str:
    r = subprocess.run(
        [sys.executable, "-X", "utf8", "-c", src],
        capture_output=True, text=True, timeout=30,
    )
    if r.returncode != 0:
        raise RuntimeError(f"reference solution failed:\n{src}\n--- stderr ---\n{r.stderr}")
    return r.stdout.replace("\r\n", "\n").rstrip("\n")


def build():
    rows = []
    for tid, prompt, ref in MICRO:
        expected = run_ref(ref)
        rows.append({
            "id": tid,
            "kind": "micro",
            "prompt": prompt + " Print exactly what is asked — no extra output.",
            "expected_stdout": expected,
        })
        print(f"[micro] {tid}: expected_stdout verified ({len(expected)} chars)")
    for tid, prompt in MACRO:
        rows.append({"id": tid, "kind": "macro", "prompt": prompt, "check": "compile"})
        print(f"[macro] {tid}: compile-check task")
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
    print(f"\nwrote {len(rows)} tasks ({len(MICRO)} micro + {len(MACRO)} macro) -> {OUT}")


def check():
    ok = True
    tasks = {json.loads(l)["id"]: json.loads(l) for l in open(OUT, encoding="utf-8")}
    for tid, _prompt, ref in MICRO:
        expected = run_ref(ref)
        if tasks[tid]["expected_stdout"] != expected:
            print(f"[FAIL] {tid}: tasks.jsonl expected_stdout is stale")
            ok = False
        else:
            print(f"[ok] {tid}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    if "--check" in sys.argv:
        check()
    else:
        build()
