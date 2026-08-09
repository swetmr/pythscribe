# Tutorial 1: Getting Started with PythScribe

This tutorial takes you from zero to running your first PythScribe program.

## Installation

```bash
# Clone and build
git clone https://github.com/pyths-lang/pyths.git
cd pyths
cargo build --release

# Add to PATH (Unix/macOS)
export PATH="$PWD/target/release:$PATH"

# Or on Windows (PowerShell)
$env:PATH = "$PWD\target\release;$env:PATH"
```

Verify:
```bash
pyths --version
```

## Hello World

Create `hello.ps`:

```python
print("Hello, World!")
```

Run it:
```bash
pyths run hello.ps
```

Output:
```
Hello, World!
```

## What Just Happened?

PythScribe compiled your `.ps` file to JavaScript, then ran it with Node.js. You can see the compiled output:

```bash
pyths compile hello.ps --stdout
```

Output:
```javascript
console.log("Hello, World!");
```

## Variables and Functions

Create `basics.ps`:

```python
# Variables
name = "PythScribe"
version = 1
is_awesome = True

# f-strings
print(f"Welcome to {name} v{version}!")

# Functions
def greet(who):
    return f"Hello, {who}!"

print(greet("developer"))

# Conditionals
score = 85
if score >= 90:
    grade = "A"
elif score >= 80:
    grade = "B"
else:
    grade = "C"

print(f"Score {score} = Grade {grade}")
```

```bash
pyths run basics.ps
```

```
Welcome to PythScribe v1!
Hello, developer!
Score 85 = Grade B
```

## Collections

```python
# collections.ps

# Lists
fruits = ["apple", "banana", "cherry"]
for fruit in fruits:
    print(f"I like {fruit}")

# List comprehension
numbers = [1, 2, 3, 4, 5]
doubled = [n * 2 for n in numbers]
evens = [n for n in numbers if n % 2 == 0]
print(f"Doubled: {doubled}")
print(f"Evens: {evens}")

# Dictionaries
person = {"name": "Alice", "age": 30, "city": "NYC"}
for key in person:
    print(f"{key}: {person[key]}")
```

## Classes

```python
# shapes.ps
class Rectangle:
    def __init__(self, width, height):
        self.width = width
        self.height = height

    def area(self):
        return self.width * self.height

    def perimeter(self):
        return 2 * (self.width + self.height)

    def __str__(self):
        return f"Rectangle({self.width}x{self.height})"

r = Rectangle(10, 5)
print(r)
print(f"Area: {r.area()}")
print(f"Perimeter: {r.perimeter()}")
```

```
Rectangle(10x5)
Area: 50
Perimeter: 30
```

## Compiling to a File

Instead of `run`, you can compile to a `.js` file:

```bash
pyths compile shapes.ps -o shapes.js
```

This creates `shapes.js` that you can run with Node.js or include in a web page:

```bash
node shapes.js
```

## Formatting and Linting

PythScribe includes built-in tools:

```bash
# Auto-format your code
pyths fmt shapes.ps

# Check for common issues
pyths lint shapes.ps
```

## Next Steps

- [Tutorial 2: Todo App](02-todo-app.md) — Build a complete todo application
- [Tutorial 3: React Components](03-react-components.md) — Build interactive UI
- [Tutorial 4: Advanced Features](04-advanced-features.md) — Dataclasses, pattern matching, generators
