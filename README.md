<div align="center">

<img src="docs/assets/logo.png" alt="rubyx-py" width="200">

# Rubyx-py

**Call Python from Ruby. No microservices, no REST APIs, no serialization overhead.**

Powered by Rust for safety and performance. Built for Rails.

[![Gem Version](https://badge.fury.io/rb/rubyx-py.svg)](https://badge.fury.io/rb/rubyx-py)
[![CI](https://github.com/yinho999/rubyx/actions/workflows/ci.yml/badge.svg)](https://github.com/yinho999/rubyx/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Ruby](https://img.shields.io/badge/Ruby-%3E%3D%203.0-red.svg)](https://www.ruby-lang.org)
[![Rust](https://img.shields.io/badge/Rust-powered-orange.svg)](https://www.rust-lang.org)

</div>

---

```ruby
np = Rubyx.import('numpy')
np.array([1, 2, 3]).mean().to_ruby # => 2.0
```

```ruby
Rubyx.eval("sum(items)", items: [1, 2, 3, 4]) # => 10
```

```ruby
# Stream LLM tokens in real-time
Rubyx.stream(llm.generate("Tell me about Ruby")).each { |token| print token }
```

```ruby
# Non-blocking — Ruby stays free while Python works
future = Rubyx.async_await("model.predict(data)", data: [1, 2, 3])
do_other_work()
result = future.await # GVL released during wait, reacquired when ready
```

### Built with non-blocking in mind

- **`Rubyx.stream`** / **`Rubyx.nb_stream`** — release Ruby's GVL during iteration, other threads and Fibers keep
  running
- **`Rubyx.async_await`** — spawns Python on background threads, returns a `Future` immediately; `future.await` releases
  the GVL while waiting, reacquires when ready
- **`Rubyx.await`** — GVL released while waiting; returns native Ruby types for primitives, `RubyxObject` for complex
  Python objects

Ideal for LLM streaming, ML inference, data pipelines, and high-concurrency Rails apps.

## Install

```ruby
# Gemfile
gem 'rubyx-py'
```

Python is auto-managed by [uv](https://github.com/astral-sh/uv). No manual install needed.

## Rails Setup Simple Example

```bash
rails generate rubyx:install
```

Creates `config/initializers/rubyx.rb`, `pyproject.toml`, and `app/python/`.

### Configuration

```ruby
# config/initializers/rubyx.rb
Rubyx::Rails.configure do |config|
  config.pyproject_path = Rails.root.join('pyproject.toml')
  config.auto_init = true
  config.python_paths = [Rails.root.join('app/python').to_s]
end
```

### Python dependencies

```toml
# pyproject.toml
[project]
name = "my-app"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

### 1. Sync — call a Python function

```python
# app/python/example.py
def hello(name):
    return f"Hello, {name}! From Python."
```

```ruby

class GreetingsController < ApplicationController
  def index
    example = Rubyx.import('example')
    render json: { message: example.hello(params[:name] || 'World').to_ruby }
  end
end
```

```ruby
Rails.application.routes.draw do
  root "greetings#index"
end
```

### 2. Streaming — iterate a Python generator

```python
# app/python/count.py
def count_up(n):
    for i in range(n):
        yield f"Step {i + 1}"
```

```ruby

class CountController < ApplicationController
  include ActionController::Live

  def stream
    response.headers['Content-Type'] = 'text/event-stream'

    counter = Rubyx.import('count')
    Rubyx.stream(counter.count_up(5)).each do |msg|
      response.stream.write("data: #{msg}\n\n")
    end
  ensure
    response.stream.close
  end
end
```

### 3. Async — non-blocking Python calls

```python
# app/python/tasks.py
import asyncio


async def delayed_greet(name, seconds=1):
    await asyncio.sleep(seconds)
    return f"Hello, {name}! (after {seconds}s)"
```

```ruby

class TasksController < ApplicationController
  def show
    tasks = Rubyx.import('tasks')

    # Non-blocking — returns a Future immediately
    future = Rubyx.async_await(tasks.delayed_greet(params[:name] || 'World', seconds: 2))
    do_other_work()
    render json: { message: future.await.to_ruby }
  end
end
```

## Rails Setup Advanced LLM Example

```bash
rails generate rubyx:install
```

Creates `config/initializers/rubyx.rb`, `pyproject.toml`, and `app/python/`.

### Configuration

```ruby
# config/initializers/rubyx.rb
Rubyx::Rails.configure do |config|
  config.pyproject_path = Rails.root.join('pyproject.toml')
  config.auto_init = true
  config.python_paths = [Rails.root.join('app/python').to_s]
end
```

### Python dependencies

```toml
# pyproject.toml
[project]
name = "my-app"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["numpy", "pandas", "transformers"]
```

### Write Python code

```python
# app/python/services/text_processing.py
class TextAnalyzer:
    def __init__(self, text):
        self.text = text
        self._words = text.split()

    def summary(self):
        return {
            "word_count": len(self._words),
            "unique_words": len(set(self._words)),
            "avg_word_length": round(
                sum(len(w) for w in self._words) / max(len(self._words), 1), 2
            ),
        }
```

### Call it from Rails

```ruby

class AnalysisController < ApplicationController
  def analyze
    tp = Rubyx.import('services.text_processing')
    analyzer = tp.TextAnalyzer(params[:text])
    render json: analyzer.summary.to_ruby
  end
end
```

### SSE streaming (LLM-style)

```python
# app/python/services/llm.py
from transformers import AutoModelForCausalLM, AutoTokenizer, TextIteratorStreamer
import threading

_model, _tokenizer = None, None


def load_model(name="Qwen/Qwen2.5-0.5B-Instruct"):
    global _model, _tokenizer
    _tokenizer = AutoTokenizer.from_pretrained(name)
    _model = AutoModelForCausalLM.from_pretrained(name, torch_dtype="auto")


def stream_generate(prompt, max_tokens=256):
    messages = [{"role": "user", "content": prompt}]
    text = _tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
    inputs = _tokenizer([text], return_tensors="pt").to(_model.device)
    streamer = TextIteratorStreamer(_tokenizer, skip_prompt=True, skip_special_tokens=True)

    thread = threading.Thread(target=_model.generate,
                              kwargs={**inputs, "max_new_tokens": max_tokens, "streamer": streamer})
    thread.start()
    for token in streamer:
        if token:
            yield token
    thread.join()
```

```ruby
# config/initializers/rubyx.rb
Rubyx::Rails.configure do |config|
  config.pyproject_path = Rails.root.join('pyproject.toml')
  config.auto_init = true
  config.python_paths = [Rails.root.join('app/python').to_s]
end

# Load models once at boot — must be inside after_initialize
# so Python is already initialized by the Railtie
Rails.application.config.after_initialize do
  llm = Rubyx.import('services.llm')
  llm.load_model("Qwen/Qwen2.5-0.5B-Instruct")
end
```

```ruby

class ChatController < ApplicationController
  include ActionController::Live

  def stream
    llm = Rubyx.import('services.llm')
    response.headers['Content-Type'] = 'text/event-stream'

    Rubyx.stream(llm.stream_generate(params[:prompt])).each do |token|
      token_str = token.to_s.gsub("\n", "\\n")
      response.stream.write("data: #{token_str}\n\n")
    end
    response.stream.write("data: [DONE]\n\n")
  ensure
    response.stream.close
  end
end
```

### Rake tasks

```bash
rake rubyx:init         # Initialize Python environment
rake rubyx:status       # Check environment status
rake rubyx:packages     # List installed Python packages
rake rubyx:clear_cache  # Clear cached environments
```

## Standalone Setup

```ruby
require 'rubyx'

Rubyx.uv_init <<~TOML
  [project]
  name = "my-script"
  version = "0.1.0"
  requires-python = ">=3.12"
  dependencies = ["numpy"]
TOML

np = Rubyx.import('numpy')
np.array([1, 2, 3, 4, 5]).std().to_ruby # => 1.4142135623730951
```

## Eval with Globals

Pass Ruby data directly into Python:

```ruby
Rubyx.eval("x ** 2 + y ** 2", x: 3, y: 4).to_ruby # => 25
Rubyx.eval("f'Hello, {name}!'", name: "World").to_ruby # => "Hello, World!"
Rubyx.eval("max(items)", items: [3, 1, 4, 1, 5]).to_ruby # => 5
```

Supports: Integer, Float, String, Symbol, Bool, nil, Array, Hash, binary String (ASCII-8BIT), and RubyxObject.

## Bytes / Binary Data

Python `bytes` and `bytearray` convert to Ruby `String` with `ASCII-8BIT` encoding. Ruby binary strings (`.b`) convert to Python `bytes`:

```ruby
# Python bytes → Ruby
Rubyx.eval("b'hello'").to_ruby # => "hello" (ASCII-8BIT)
Rubyx.eval("bytearray(b'hello')").to_ruby # => "hello" (ASCII-8BIT)

# Ruby → Python
ctx = Rubyx.context
ctx.eval("type(data).__name__", data: "hello".b) # => "bytes"
ctx.eval("type(data).__name__", data: "hello")   # => "str"

# Roundtrip binary data
ctx.eval("data", data: "\xff\x00\xfe".b).to_ruby # => "\xFF\x00\xFE" (ASCII-8BIT)

# Works with Python stdlib
ctx.eval("import base64")
ctx.eval("base64.b64encode(raw)", raw: "Hello".b).to_ruby # => "SGVsbG8=" (ASCII-8BIT)
```

## Python Objects

Python objects are wrapped as `RubyxObject`:

```ruby
os = Rubyx.import('os')
os.getcwd().to_ruby # => "/home/user"
os.path.exists("/tmp").to_ruby # => true

# Subscript access
d = Rubyx.eval("{'a': 1, 'b': 2}")
d['a'].to_ruby # => 1
d['c'] = 3

# Enumerable
py_list = Rubyx.eval("[1, 2, 3, 4, 5]")
py_list.map { |x| x.to_ruby * 2 } # => [2, 4, 6, 8, 10]
py_list.select { |x| x.to_ruby > 3 } # filtered RubyxObjects

# Introspection
py_list.truthy? # => true
py_list.callable? # => false
py_list.py_type # => "list"
```

## Context

Persistent state across multiple eval calls:

```ruby
ctx = Rubyx.context

ctx.eval("import math")
ctx.eval("data = [1, 2, 3, 4, 5]")
ctx.eval("avg = sum(data) / len(data)")
ctx.eval("avg").to_ruby # => 3.0

# Inject Ruby data into context
ctx.eval("total = base + offset", base: 100, offset: 42)
ctx.eval("total").to_ruby # => 142
```

## Streaming

```ruby
gen = Rubyx.eval("(x ** 2 for x in range(5))")
Rubyx.stream(gen).each { |val| puts val } # 0, 1, 4, 9, 16

# Non-blocking (releases GVL for other Ruby threads)
Rubyx.nb_stream(gen).each { |val| process(val) }
```

## Async / Await

```ruby
ctx = Rubyx.context
ctx.eval("import asyncio")
ctx.eval("async def fetch(url): ...")

# GVL released while waiting, reacquired when ready
result = ctx.await("fetch(url)", url: "https://example.com")

# Non-blocking (returns Future)
future = ctx.async_await("fetch(url)", url: "https://example.com")
do_other_stuff()
result = future.await #  GVL released during wait, reacquired when ready
future.ready? # check without blocking
```

## Error Handling

Python exceptions map to Ruby classes:

```ruby

begin
  Rubyx.eval('{}["missing"]')
rescue Rubyx::KeyError => e
  puts e.message # includes Python traceback
end
```

| Python                                | Ruby                          |
|---------------------------------------|-------------------------------|
| `KeyError`                            | `Rubyx::KeyError`             |
| `IndexError`                          | `Rubyx::IndexError`           |
| `ValueError`                          | `Rubyx::ValueError`           |
| `TypeError`                           | `Rubyx::TypeError`            |
| `AttributeError`                      | `Rubyx::AttributeError`       |
| `ImportError` / `ModuleNotFoundError` | `Rubyx::ImportError`          |
| `SyntaxError`                         | `SyntaxError` (Ruby built-in) |
| Everything else                       | `Rubyx::PythonError`          |

All inherit from `Rubyx::Error` (`StandardError`).

## Local Python Files

```python
# app/python/services/analyzer.py
class Analyzer:
    def __init__(self, data):
        self.data = data

    def summary(self):
        return {"count": len(self.data), "sum": sum(self.data)}
```

```ruby
svc = Rubyx.import('services.analyzer')
svc.Analyzer([1, 2, 3]).summary.to_ruby # => {"count" => 3, "sum" => 6}
```

## API Reference

| Method                               | Description                                   |
|--------------------------------------|-----------------------------------------------|
| `Rubyx.uv_init(toml, **opts)`        | Setup Python env and initialize               |
| `Rubyx.import(name)`                 | Import a Python module                        |
| `Rubyx.eval(code, **globals)`        | Evaluate Python code                          |
| `Rubyx.await(code, **globals)`       | Run async code (GVL released while waiting)   |
| `Rubyx.async_await(code, **globals)` | Run async code (non-blocking, returns Future) |
| `Rubyx.stream(iterable)`             | Stream a Python generator                     |
| `Rubyx.nb_stream(iterable)`          | Non-blocking stream (GVL-aware)               |
| `Rubyx.context`                      | Create isolated Python context                |
| `Rubyx.initialized?`                 | Check if Python is ready                      |

| RubyxObject              |                               |
|--------------------------|-------------------------------|
| `.to_ruby`               | Convert to native Ruby type   |
| `.to_s` / `.inspect`     | String / repr                 |
| `.method_missing`        | Delegates to Python           |
| `[]` / `[]=` / `.delete` | Subscript access              |
| `.each`                  | Iterate (includes Enumerable) |
| `.truthy?` / `.falsy?`   | Python truthiness             |
| `.callable?`             | Check if callable             |
| `.py_type`               | Python type name              |

## Type Conversion

| Python                | Ruby                         | Notes                  |
|-----------------------|------------------------------|------------------------|
| `int`                 | `Integer`                    |                        |
| `float`               | `Float`                      |                        |
| `str`                 | `String` (UTF-8)             |                        |
| `bytes`               | `String` (ASCII-8BIT)        | binary data            |
| `bytearray`           | `String` (ASCII-8BIT)        | binary data            |
| `bool`                | `true` / `false`             |                        |
| `None`                | `nil`                        |                        |
| `list` / `tuple`      | `Array`                      |                        |
| `dict`                | `Hash`                       |                        |
| `set` / `frozenset`   | `Array`                      |                        |
| everything else       | `RubyxObject`                | proxy to Python object |

**Ruby → Python** (via globals/kwargs):

| Ruby                           | Python      |
|--------------------------------|-------------|
| `Integer`                      | `int`       |
| `Float`                        | `float`     |
| `String` (UTF-8)               | `str`       |
| `String` (ASCII-8BIT / `.b`)   | `bytes`     |
| `true` / `false`               | `bool`      |
| `nil`                          | `None`      |
| `Array`                        | `list`      |
| `Hash`                         | `dict`      |
| `Symbol`                       | `str`       |
| `RubyxObject`                  | original    |

## Requirements

- Ruby >= 3.0
- Rust (for building from source)
- Python >= 3.12 (auto-managed by uv)

Precompiled gems available for Linux and macOS (x86_64 and ARM64).

## License

MIT
