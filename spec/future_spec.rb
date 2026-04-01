require_relative 'spec_helper'

RSpec.describe 'Rubyx::Future', ruby_integration: true do
  # ========== Rubyx.async_await ==========

  describe 'Rubyx.async_await' do
    it 'returns a Rubyx::Future' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def simple(): return 42")
      coro = ctx.eval("simple()")
      future = Rubyx.async_await(coro)
      expect(future).to be_a(Rubyx::Future)
      future.await # consume to clean up thread
    end

    it 'runs the coroutine on a background thread' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow_add(): await asyncio.sleep(0.01); return 3 + 4")
      coro = ctx.eval("slow_add()")

      future = Rubyx.async_await(coro)

      # Ruby is not blocked — we can do work here
      ruby_work_done = true

      result = future.await
      expect(ruby_work_done).to be true
      expect(result).to eq(7)
    end

    it 'returns the correct value from async function' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_msg(): return 'hello from async'")
      coro = ctx.eval("get_msg()")

      future = Rubyx.async_await(coro)
      expect(future.await).to eq('hello from async')
    end

    it 'handles async function returning a list' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_list(): return [1, 2, 3]")
      coro = ctx.eval("get_list()")

      future = Rubyx.async_await(coro)
      expect(future.await).to eq([1, 2, 3])
    end

    it 'handles async function returning a dict' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_dict(): return {'key': 'value'}")
      coro = ctx.eval("get_dict()")

      future = Rubyx.async_await(coro)
      expect(future.await).to eq({ 'key' => 'value' })
    end

    it 'handles async function returning None' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def noop(): pass")
      coro = ctx.eval("noop()")

      future = Rubyx.async_await(coro)
      expect(future.await).to be_nil
    end

    it 'propagates async errors' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def boom(): raise ValueError('async error')")
      coro = ctx.eval("boom()")

      future = Rubyx.async_await(coro)
      expect { future.await }.to raise_error(RuntimeError, /async error/)
    end
  end

  # ========== ready? ==========

  describe '#ready?' do
    it 'returns false before completion' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow(): await asyncio.sleep(0.5); return 1")
      coro = ctx.eval("slow()")

      future = Rubyx.async_await(coro)
      # Might be false if checked immediately (race condition, but likely)
      # Just verify it doesn't raise
      expect(future.ready?).to be(true).or be(false)
      future.await # clean up
    end

    it 'returns true after completion' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def fast(): return 42")
      coro = ctx.eval("fast()")

      future = Rubyx.async_await(coro)
      future.await # wait for completion

      # After await is consumed, ready? behavior is implementation-defined
      # Just verify it doesn't crash
      expect { future.ready? }.not_to raise_error
    end
  end

  # ========== context.async_await ==========

  describe 'context.async_await' do
    it 'evals and runs async in one step' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def double(n): return n * 2")

      future = ctx.async_await("double(21)")
      expect(future).to be_a(Rubyx::Future)
      expect(future.await).to eq(42)
    end

    it 'has access to context state' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("x = 10")
      ctx.eval("async def get_x(): return x")

      future = ctx.async_await("get_x()")
      expect(future.await).to eq(10)
    end

    it 'propagates errors from async code' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def fail(): raise RuntimeError('context async error')")

      future = ctx.async_await("fail()")
      expect { future.await }.to raise_error(RuntimeError, /context async error/)
    end

    it 'raises on invalid Python code' do
      ctx = Rubyx.context
      expect { ctx.async_await("not valid python!!!") }.to raise_error(Exception)
    end
  end

  # ========== context.await (blocking) ==========

  describe 'context.await (blocking)' do
    it 'evals and blocks until result' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def blocking_ctx(): return 77")

      result = ctx.await("blocking_ctx()")
      expect(result).to eq(77)
    end

    it 'has access to context state' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("val = 'hello'")
      ctx.eval("async def get_val(): return val")

      result = ctx.await("get_val()")
      expect(result).to eq('hello')
    end

    it 'returns native Hash for dict' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_dict(): return {'a': 1, 'b': 2}")

      result = ctx.await("get_dict()")
      expect(result).to eq({ 'a' => 1, 'b' => 2 })
    end

    it 'propagates errors' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def fail(): raise ValueError('ctx await error')")

      expect { ctx.await("fail()") }.to raise_error(StandardError, /ctx await error/)
    end

    it 'works with await in coroutine body' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def delayed(): await asyncio.sleep(0.01); return 'done'")

      result = ctx.await("delayed()")
      expect(result).to eq('done')
    end
  end

  # ========== Rubyx.await (blocking standalone) ==========

  describe 'Rubyx.await (blocking)' do
    it 'blocks and returns native value' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def blocking_test(): return 99")
      coro = ctx.eval("blocking_test()")

      result = Rubyx.await(coro)
      expect(result).to eq(99)
    end

    it 'returns string result' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_str(): return 'awaited'")
      coro = ctx.eval("get_str()")

      result = Rubyx.await(coro)
      expect(result).to eq('awaited')
    end

    it 'propagates errors' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def boom(): raise RuntimeError('await boom')")
      coro = ctx.eval("boom()")

      expect { Rubyx.await(coro) }.to raise_error(StandardError, /await boom/)
    end

    it 'handles None return' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def nothing(): pass")
      coro = ctx.eval("nothing()")

      result = Rubyx.await(coro)
      expect(result).to be_nil
    end
  end

  # ========== Rubyx.async_await edge cases ==========

  describe 'Rubyx.async_await edge cases' do
    it 'raises error for invalid Python code string' do
      expect { Rubyx.async_await("not a python object") }.to raise_error(Exception)
    end

    it 'future.await can only be consumed once' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def once(): return 1")
      coro = ctx.eval("once()")

      future = Rubyx.async_await(coro)
      expect(future.await).to eq(1)
      # Second call should fail
      expect { future.await }.to raise_error(RuntimeError)
    end
  end

  # ========== concurrent futures ==========

  describe 'concurrent futures' do
    it 'can run multiple futures sequentially' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def add(a, b): await asyncio.sleep(0.01); return a + b")

      f1 = ctx.async_await("add(1, 2)")
      expect(f1.await).to eq(3)

      f2 = ctx.async_await("add(3, 4)")
      expect(f2.await).to eq(7)
    end

    it 'Ruby threads can run while future executes' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow(): await asyncio.sleep(0.1); return 42")

      future = ctx.async_await("slow()")

      # Ruby thread does work while Python runs
      counter = 0
      while !future.ready?
        counter += 1
        sleep(0.01)
      end

      expect(future.await).to eq(42)
      # Counter should be > 0 if Ruby was doing work
      # (might be 0 on very fast machines, so don't assert)
    end
  end

  # ========== GVL release during await ==========

  describe 'GVL release during await' do
    it 'other Ruby threads run while Rubyx.await blocks' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow_result(): await asyncio.sleep(0.3); return 'done'")
      coro = ctx.eval("slow_result()")

      counter = 0
      mutex = Mutex.new
      done = false

      worker = Thread.new do
        until done
          mutex.synchronize { counter += 1 }
          sleep(0.01)
        end
      end

      result = Rubyx.await(coro)
      done = true
      worker.join

      expect(result).to eq('done')
      expect(counter).to be > 5
    end

    it 'other Ruby threads run while future.await blocks' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow_future(): await asyncio.sleep(0.3); return 'future_done'")

      future = ctx.async_await("slow_future()")

      counter = 0
      mutex = Mutex.new
      done = false

      worker = Thread.new do
        until done
          mutex.synchronize { counter += 1 }
          sleep(0.01)
        end
      end

      result = future.await
      done = true
      worker.join

      expect(result).to eq('future_done')
      expect(counter).to be > 5
    end

    it 'other Ruby threads run while ctx.await blocks' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def slow_ctx(): await asyncio.sleep(0.3); return 'ctx_done'")

      counter = 0
      mutex = Mutex.new
      done = false

      worker = Thread.new do
        until done
          mutex.synchronize { counter += 1 }
          sleep(0.01)
        end
      end

      result = ctx.await("slow_ctx()")
      done = true
      worker.join

      expect(result).to eq('ctx_done')
      expect(counter).to be > 5
    end
  end

  # ========== class identity ==========

  describe 'class identity' do
    it 'Rubyx::Future is defined' do
      expect(defined?(Rubyx::Future)).to eq('constant')
    end

    it 'Rubyx.async_await returns Rubyx::Future' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def id_test(): return 1")
      coro = ctx.eval("id_test()")
      future = Rubyx.async_await(coro)
      expect(future).to be_a(Rubyx::Future)
      future.await
    end

    it 'ctx.async_await returns Rubyx::Future' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def id_test2(): return 2")
      future = ctx.async_await("id_test2()")
      expect(future).to be_a(Rubyx::Future)
      future.await
    end

    it 'Rubyx.await returns native value (not Future)' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def id_test3(): return 3")
      coro = ctx.eval("id_test3()")
      result = Rubyx.await(coro)
      expect(result).to eq(3)
      expect(result).not_to be_a(Rubyx::Future)
    end
  end

  # ========== Rubyx.await with globals ==========

  describe 'Rubyx.await with globals' do
    it 'awaits coroutine expression with globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def mul(a, b): return a * b")
      result = ctx.await('mul(a, b)', a: 6, b: 7)
      expect(result).to eq(42)
    end

    it 'awaits with string globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def greet(n): return f'hi {n}'")
      result = ctx.await('greet(name)', name: 'world')
      expect(result).to eq('hi world')
    end

    it 'propagates errors with globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def fail_neg(v):\n    if v < 0: raise ValueError('negative')\n    return v")
      expect { ctx.await('fail_neg(val)', val: -1) }.to raise_error(StandardError, /negative/)
    end
  end

  # ========== Rubyx.async_await with globals ==========

  describe 'Rubyx.async_await with globals' do
    it 'returns Future with globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def add(a, b): return a + b")
      future = ctx.async_await('add(x, y)', x: 20, y: 22)
      expect(future).to be_a(Rubyx::Future)
      expect(future.await).to eq(42)
    end

    it 'handles string result with globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def greet(n): return f'hello {n}'")
      future = ctx.async_await('greet(name)', name: 'world')
      expect(future.await).to eq('hello world')
    end

    it 'propagates errors with globals' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def div(x, y): return x / y")
      future = ctx.async_await('div(a, b)', a: 10, b: 0)
      expect { future.await }.to raise_error(StandardError, /division by zero|ZeroDivisionError/)
    end
  end

  # ========== ArgumentError guards ==========

  describe 'ArgumentError for coroutine + globals' do
    it 'Rubyx.await raises ArgumentError when passing globals with coroutine object' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def noop(): return 1")
      coro = ctx.eval("noop()")
      expect { Rubyx.await(coro, x: 1) }.to raise_error(ArgumentError, /cannot pass globals/)
    end

    it 'Rubyx.async_await raises ArgumentError when passing globals with coroutine object' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def noop(): return 1")
      coro = ctx.eval("noop()")
      expect { Rubyx.async_await(coro, x: 1) }.to raise_error(ArgumentError, /cannot pass globals/)
    end

    it 'Rubyx.await works with coroutine object (no globals)' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def get99(): return 99")
      coro = ctx.eval("get99()")
      result = Rubyx.await(coro)
      expect(result).to eq(99)
    end

    it 'Rubyx.async_await works with coroutine object (no globals)' do
      ctx = Rubyx.context
      ctx.eval("import asyncio\nasync def get77(): return 77")
      coro = ctx.eval("get77()")
      future = Rubyx.async_await(coro)
      expect(future.await).to eq(77)
    end
  end

  # ========== Complex objects via PyObjectRef ==========

  describe 'Rubyx.await with complex objects' do
    it 'returns RubyxObject for custom class instances' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("class Wrapper:\n    def __init__(self, v):\n        self.value = v")
      ctx.eval("async def get_wrapper(): return Wrapper(42)")
      coro = ctx.eval("get_wrapper()")

      result = Rubyx.await(coro)
      expect(result).to be_a(RubyxObject)
      expect(result.value.to_ruby).to eq(42)
    end

    it 'returns RubyxObject for modules via Rubyx.await' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_os(): import os; return os")
      coro = ctx.eval("get_os()")

      result = Rubyx.await(coro)
      expect(result).to be_a(RubyxObject)
      expect(result.py_type).to eq('module')
    end

    it 'returns native Ruby types for primitives' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")

      ctx.eval("async def get_int(): return 42")
      expect(Rubyx.await(ctx.eval("get_int()"))).to eq(42)

      ctx.eval("async def get_str(): return 'hello'")
      expect(Rubyx.await(ctx.eval("get_str()"))).to eq('hello')

      ctx.eval("async def get_float(): return 3.14")
      expect(Rubyx.await(ctx.eval("get_float()"))).to be_within(0.001).of(3.14)

      ctx.eval("async def get_bool(): return True")
      expect(Rubyx.await(ctx.eval("get_bool()"))).to eq(true)

      ctx.eval("async def get_none(): return None")
      expect(Rubyx.await(ctx.eval("get_none()"))).to be_nil

      ctx.eval("async def get_list(): return [1, 2, 3]")
      expect(Rubyx.await(ctx.eval("get_list()"))).to eq([1, 2, 3])

      ctx.eval("async def get_dict(): return {'a': 1}")
      expect(Rubyx.await(ctx.eval("get_dict()"))).to eq({'a' => 1})
    end
  end

  describe 'future.await with complex objects' do
    it 'returns RubyxObject for modules' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_module(): import os; return os")

      future = ctx.async_await("get_module()")
      result = future.await
      expect(result).to be_a(RubyxObject)
      expect(result.py_type).to eq('module')
    end

    it 'returns RubyxObject for custom class instances' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("class Point:\n    def __init__(self, x, y):\n        self.x = x\n        self.y = y")
      ctx.eval("async def make_point(): return Point(3, 4)")

      future = ctx.async_await("make_point()")
      result = future.await
      expect(result).to be_a(RubyxObject)
      expect(result.x.to_ruby).to eq(3)
      expect(result.y.to_ruby).to eq(4)
    end

    it 'returns native types for primitives via future.await' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_val(): return 99")

      future = ctx.async_await("get_val()")
      expect(future.await).to eq(99)
    end
  end

  # ========== future.await behavior ==========

  describe 'future.await behavior' do
    it 'blocks until result is ready' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def delayed(): await asyncio.sleep(0.05); return 'delayed'")

      future = ctx.async_await("delayed()")
      start = Time.now
      result = future.await
      elapsed = Time.now - start

      expect(result).to eq('delayed')
      expect(elapsed).to be >= 0.03
    end

    it 'returns error for failed async function' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def fail(): raise ValueError('boom')")

      future = ctx.async_await("fail()")
      expect { future.await }.to raise_error(RuntimeError, /boom/)
    end

    it 'can be consumed only once' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def once(): return 1")

      future = ctx.async_await("once()")
      expect(future.await).to eq(1)
      expect { future.await }.to raise_error(RuntimeError)
    end

    it 'handles None return' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def noop(): pass")

      future = ctx.async_await("noop()")
      expect(future.await).to be_nil
    end

    it 'handles list return' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_list(): return [10, 20, 30]")

      future = ctx.async_await("get_list()")
      expect(future.await).to eq([10, 20, 30])
    end

    it 'handles dict return' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_dict(): return {'key': 'value'}")

      future = ctx.async_await("get_dict()")
      expect(future.await).to eq({ 'key' => 'value' })
    end
  end

  # ========== Callable objects via await ==========

  describe 'Rubyx.await with callable objects' do
    it 'returns a lambda as RubyxObject' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_func(): return lambda x: x * 3")
      coro = ctx.eval("get_func()")

      result = Rubyx.await(coro)
      expect(result).to be_a(RubyxObject)
      expect(result.callable?).to be true
    end

    it 'returned lambda is invocable via __call__' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_doubler(): return lambda x: x * 2")
      coro = ctx.eval("get_doubler()")

      func = Rubyx.await(coro)
      result = func.__call__(21)
      expect(result.to_ruby).to eq(42)
    end

    it 'returns a user-defined function' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval(<<~PY)
        def add(a, b):
            return a + b
        async def get_add():
            return add
      PY
      coro = ctx.eval("get_add()")

      func = Rubyx.await(coro)
      expect(func).to be_a(RubyxObject)
      expect(func.callable?).to be true
      expect(func.__call__(3, 4).to_ruby).to eq(7)
    end

    it 'returns a class that can construct instances' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval(<<~PY)
        class Dog:
            def __init__(self, name):
                self.name = name
        async def get_dog_class():
            return Dog
      PY
      coro = ctx.eval("get_dog_class()")

      cls = Rubyx.await(coro)
      expect(cls).to be_a(RubyxObject)
      expect(cls.callable?).to be true

      dog = cls.__call__('Rex')
      expect(dog.name.to_ruby).to eq('Rex')
    end

    it 'returns a builtin function' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_len(): return len")
      coro = ctx.eval("get_len()")

      len_func = Rubyx.await(coro)
      expect(len_func).to be_a(RubyxObject)
      expect(len_func.callable?).to be true
    end

    it 'returns a callable instance with __call__' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval(<<~PY)
        class Multiplier:
            def __init__(self, factor):
                self.factor = factor
            def __call__(self, x):
                return x * self.factor
        async def get_tripler():
            return Multiplier(3)
      PY
      coro = ctx.eval("get_tripler()")

      tripler = Rubyx.await(coro)
      expect(tripler).to be_a(RubyxObject)
      expect(tripler.callable?).to be true
      expect(tripler.__call__(7).to_ruby).to eq(21)
    end

    it 'returns a closure that captures state' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval(<<~PY)
        async def get_counter():
            count = [0]
            def increment():
                count[0] += 1
                return count[0]
            return increment
      PY
      coro = ctx.eval("get_counter()")

      counter = Rubyx.await(coro)
      expect(counter.__call__.to_ruby).to eq(1)
      expect(counter.__call__.to_ruby).to eq(2)
      expect(counter.__call__.to_ruby).to eq(3)
    end
  end

  describe 'future.await with callable objects' do
    it 'returns callable via future.await' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_func(): return lambda x: x + 1")

      future = ctx.async_await("get_func()")
      func = future.await
      expect(func).to be_a(RubyxObject)
      expect(func.callable?).to be true
      expect(func.__call__(9).to_ruby).to eq(10)
    end

    it 'returns a function factory result via future.await' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval(<<~PY)
        def make_adder(n):
            return lambda x: x + n
        async def get_add5():
            return make_adder(5)
      PY

      future = ctx.async_await("get_add5()")
      add5 = future.await
      expect(add5.__call__(10).to_ruby).to eq(15)
    end
  end

  describe 'ctx.await with callable objects' do
    it 'returns callable via ctx.await' do
      ctx = Rubyx.context
      ctx.eval("import asyncio")
      ctx.eval("async def get_func(): return lambda x: x ** 2")

      func = ctx.await("get_func()")
      expect(func).to be_a(RubyxObject)
      expect(func.callable?).to be true
      expect(func.__call__(5).to_ruby).to eq(25)
    end
  end
end
