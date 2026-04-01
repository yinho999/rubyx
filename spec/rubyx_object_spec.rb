require_relative 'spec_helper'

RSpec.describe 'RubyxObject', ruby_integration: true do
  # ========== to_s ==========

  describe '#to_s' do
    it 'returns string representation of integer' do
      obj = Rubyx.eval('42')
      expect(obj.to_s).to eq('42')
    end

    it 'returns string value for Python string' do
      obj = Rubyx.eval('"hello world"')
      expect(obj.to_s).to eq('hello world')
    end

    it 'returns "None" for Python None' do
      obj = Rubyx.eval('None')
      expect(obj.to_s).to eq('None')
    end

    it 'returns "True" for Python True' do
      obj = Rubyx.eval('True')
      expect(obj.to_s).to eq('True')
    end

    it 'returns "False" for Python False' do
      obj = Rubyx.eval('False')
      expect(obj.to_s).to eq('False')
    end

    it 'returns float string' do
      obj = Rubyx.eval('3.14')
      expect(obj.to_s).to start_with('3.14')
    end

    it 'returns list representation' do
      obj = Rubyx.eval('[1, 2, 3]')
      expect(obj.to_s).to eq('[1, 2, 3]')
    end

    it 'returns dict representation' do
      obj = Rubyx.eval("{'a': 1}")
      expect(obj.to_s).to eq("{'a': 1}")
    end

    it 'works with print' do
      obj = Rubyx.eval('42')
      expect { print obj }.to output('42').to_stdout
    end

    it 'works with puts' do
      obj = Rubyx.eval('42')
      expect { puts obj }.to output("42\n").to_stdout
    end

    it 'works with puts for string' do
      obj = Rubyx.eval('"hello"')
      expect { puts obj }.to output("hello\n").to_stdout
    end

    it 'works with puts for None' do
      obj = Rubyx.eval('None')
      expect { puts obj }.to output("None\n").to_stdout
    end

    it 'works with puts for list' do
      obj = Rubyx.eval('[1, 2, 3]')
      expect { puts obj }.to output("[1, 2, 3]\n").to_stdout
    end

    it 'works with string interpolation' do
      obj = Rubyx.eval('"world"')
      expect("hello #{obj}").to eq('hello world')
    end
  end

  # ========== inspect ==========

  describe '#inspect' do
    it 'returns repr for integer' do
      obj = Rubyx.eval('42')
      expect(obj.inspect).to eq('42')
    end

    it 'returns repr for string (with quotes)' do
      obj = Rubyx.eval('"hello"')
      expect(obj.inspect).to eq("'hello'")
    end

    it 'returns repr for None' do
      obj = Rubyx.eval('None')
      expect(obj.inspect).to eq('None')
    end

    it 'returns repr for list' do
      obj = Rubyx.eval('[1, 2, 3]')
      expect(obj.inspect).to eq('[1, 2, 3]')
    end

    it 'differs from to_s for strings' do
      obj = Rubyx.eval('"test"')
      expect(obj.to_s).to eq('test')       # Python str()
      expect(obj.inspect).to eq("'test'")  # Python repr()
    end
  end

  # ========== to_ruby ==========

  describe '#to_ruby' do
    it 'converts Python int to Ruby Integer' do
      obj = Rubyx.eval('42')
      result = obj.to_ruby
      expect(result).to eq(42)
      expect(result).to be_a(Integer)
    end

    it 'converts Python float to Ruby Float' do
      obj = Rubyx.eval('3.14')
      result = obj.to_ruby
      expect(result).to be_within(0.001).of(3.14)
      expect(result).to be_a(Float)
    end

    it 'converts Python str to Ruby String' do
      obj = Rubyx.eval('"hello"')
      result = obj.to_ruby
      expect(result).to eq('hello')
      expect(result).to be_a(String)
    end

    it 'converts Python True to Ruby true' do
      obj = Rubyx.eval('True')
      result = obj.to_ruby
      expect(result).to eq(true)
    end

    it 'converts Python False to Ruby false' do
      obj = Rubyx.eval('False')
      result = obj.to_ruby
      expect(result).to eq(false)
    end

    it 'converts Python None to Ruby nil' do
      obj = Rubyx.eval('None')
      result = obj.to_ruby
      expect(result).to be_nil
    end

    it 'converts Python list to Ruby Array' do
      obj = Rubyx.eval('[1, 2, 3]')
      result = obj.to_ruby
      expect(result).to eq([1, 2, 3])
      expect(result).to be_a(Array)
    end

    it 'converts Python dict to Ruby Hash' do
      obj = Rubyx.eval("{'key': 'value', 'num': 42}")
      result = obj.to_ruby
      expect(result).to be_a(Hash)
      expect(result['key']).to eq('value')
      expect(result['num']).to eq(42)
    end

    it 'converts nested structures' do
      obj = Rubyx.eval("{'items': [1, 2, 3], 'nested': {'a': True}}")
      result = obj.to_ruby
      expect(result['items']).to eq([1, 2, 3])
      expect(result['nested']).to eq({ 'a' => true })
    end

    it 'wraps modules as RubyxObject via PyObjectRef' do
      obj = Rubyx.import('os')
      result = obj.to_ruby
      expect(result).to be_a(RubyxObject)
    end
  end

  # ========== method_missing ==========

  describe '#method_missing' do
    it 'calls Python methods on imported modules' do
      ctx = Rubyx.context
      ctx.eval('import json')
      json_mod = ctx.eval('json')
      result = json_mod.loads('[1, 2, 3]')
      expect(result).to be_a(RubyxObject)
      expect(result.to_ruby).to eq([1, 2, 3])
    end

    it 'reads Python attributes' do
      ctx = Rubyx.context
      ctx.eval('import sys')
      sys_mod = ctx.eval('sys')
      version = sys_mod.version
      expect(version).to be_a(RubyxObject)
      expect(version.to_s).not_to be_empty
    end

    it 'raises for nonexistent attributes' do
      obj = Rubyx.import('sys')
      expect { obj.nonexistent_attr_xyz_123 }.to raise_error(Exception)
    end

    it 'calls methods with string arguments' do
      ctx = Rubyx.context
      ctx.eval("import json")
      json_mod = ctx.eval("json")
      result = json_mod.loads('[1, 2, 3]')
      expect(result.to_ruby).to eq([1, 2, 3])
    end
  end

  # ========== respond_to? ==========

  describe '#respond_to?' do
    it 'returns true for existing Python attributes' do
      sys = Rubyx.import('sys')
      expect(sys.respond_to?(:version)).to be true
    end

    it 'returns false for nonexistent attributes' do
      sys = Rubyx.import('sys')
      expect(sys.respond_to?(:nonexistent_xyz_123)).to be false
    end

    it 'returns true for callable methods' do
      ctx = Rubyx.context
      ctx.eval("import json")
      json_mod = ctx.eval("json")
      expect(json_mod.respond_to?(:loads)).to be true
      expect(json_mod.respond_to?(:dumps)).to be true
    end

    it 'returns true for to_s (Ruby method)' do
      obj = Rubyx.eval('42')
      expect(obj.respond_to?(:to_s)).to be true
    end

    it 'returns true for inspect (Ruby method)' do
      obj = Rubyx.eval('42')
      expect(obj.respond_to?(:inspect)).to be true
    end

    it 'returns true for to_ruby (Ruby method)' do
      obj = Rubyx.eval('42')
      expect(obj.respond_to?(:to_ruby)).to be true
    end
  end

  # ========== implicit conversion guards ==========

  describe 'implicit conversion guards' do
    it 'does not delegate to_ary to Python' do
      obj = Rubyx.eval('42')
      expect { obj.to_ary }.to raise_error(NoMethodError)
    end

    it 'does not delegate to_str to Python' do
      obj = Rubyx.eval('42')
      expect { obj.to_str }.to raise_error(NoMethodError)
    end

    it 'does not delegate to_hash to Python' do
      obj = Rubyx.eval('42')
      expect { obj.to_hash }.to raise_error(NoMethodError)
    end

    it 'does not delegate to_int to Python' do
      obj = Rubyx.eval('42')
      expect { obj.to_int }.to raise_error(NoMethodError)
    end

    it 'does not delegate to_float to Python' do
      obj = Rubyx.eval('42')
      expect { obj.to_float }.to raise_error(NoMethodError)
    end
  end

  # ========== [] / []= / delete ==========

  describe '[] (getitem)' do
    it 'reads dict values by string key' do
      d = Rubyx.eval("{'name': 'Alice', 'age': 30}")
      expect(d["name"].to_ruby).to eq("Alice")
      expect(d["age"].to_ruby).to eq(30)
    end

    it 'reads list values by integer index' do
      l = Rubyx.eval("[10, 20, 30]")
      expect(l[0].to_ruby).to eq(10)
      expect(l[1].to_ruby).to eq(20)
      expect(l[2].to_ruby).to eq(30)
    end

    it 'supports negative indexing on lists' do
      l = Rubyx.eval("[10, 20, 30]")
      expect(l[-1].to_ruby).to eq(30)
      expect(l[-2].to_ruby).to eq(20)
    end

    it 'raises for missing dict key' do
      d = Rubyx.eval("{}")
      expect { d["nope"] }.to raise_error(StandardError)
    end

    it 'raises for out-of-range list index' do
      l = Rubyx.eval("[1, 2]")
      expect { l[99] }.to raise_error(StandardError)
    end

    it 'returns RubyxObject' do
      d = Rubyx.eval("{'x': 42}")
      expect(d["x"]).to be_a(RubyxObject)
    end
  end

  describe '[]= (setitem)' do
    it 'sets dict values' do
      d = Rubyx.eval("{}")
      d["name"] = "Bob"
      expect(d["name"].to_ruby).to eq("Bob")
    end

    it 'overwrites existing dict values' do
      d = Rubyx.eval("{'x': 1}")
      d["x"] = 42
      expect(d["x"].to_ruby).to eq(42)
    end

    it 'sets list values by index' do
      l = Rubyx.eval("[1, 2, 3]")
      l[1] = 99
      expect(l[1].to_ruby).to eq(99)
    end

    it 'returns the assigned value' do
      d = Rubyx.eval("{}")
      result = (d["key"] = "value")
      expect(result).to eq("value")
    end
  end

  describe '#delete (delitem)' do
    it 'removes dict keys' do
      d = Rubyx.eval("{'a': 1, 'b': 2}")
      d.delete("a")
      expect { d["a"] }.to raise_error(StandardError)
    end

    it 'leaves other keys intact after delete' do
      d = Rubyx.eval("{'a': 1, 'b': 2}")
      d.delete("a")
      expect(d["b"].to_ruby).to eq(2)
    end

    it 'raises for missing key' do
      d = Rubyx.eval("{}")
      expect { d.delete("nope") }.to raise_error(StandardError)
    end
  end

  # ========== each / Enumerable ==========

  describe '#each' do
    it 'iterates over a list' do
      l = Rubyx.eval("[10, 20, 30]")
      values = []
      l.each { |item| values << item.to_ruby }
      expect(values).to eq([10, 20, 30])
    end

    it 'iterates over an empty list' do
      l = Rubyx.eval("[]")
      values = []
      l.each { |item| values << item }
      expect(values).to be_empty
    end

    it 'iterates over dict keys' do
      d = Rubyx.eval("{'a': 1, 'b': 2}")
      keys = []
      d.each { |key| keys << key.to_ruby }
      expect(keys).to contain_exactly('a', 'b')
    end

    it 'iterates over a string (characters)' do
      s = Rubyx.eval('"abc"')
      chars = []
      s.each { |c| chars << c.to_ruby }
      expect(chars).to eq(%w[a b c])
    end

    it 'yields RubyxObject items' do
      l = Rubyx.eval("[1, 2]")
      l.each { |item| expect(item).to be_a(RubyxObject) }
    end

    it 'returns Enumerator when no block given' do
      l = Rubyx.eval("[1, 2, 3]")
      enum = l.each
      expect(enum).to be_a(Enumerator)
    end

    it 'raises TypeError for non-iterable objects' do
      obj = Rubyx.eval('42')
      expect { obj.each {} }.to raise_error(StandardError, /not iterable/)
    end
  end

  describe 'Enumerable methods' do
    it '#map transforms values' do
      l = Rubyx.eval("[1, 2, 3]")
      result = l.map { |item| item.to_ruby * 10 }
      expect(result).to eq([10, 20, 30])
    end

    it '#select filters values' do
      l = Rubyx.eval("[1, 2, 3, 4, 5]")
      result = l.select { |item| item.to_ruby > 3 }
      expect(result.map(&:to_ruby)).to eq([4, 5])
    end

    it '#to_a collects all items' do
      l = Rubyx.eval("[10, 20, 30]")
      arr = l.to_a
      expect(arr.length).to eq(3)
      expect(arr.map(&:to_ruby)).to eq([10, 20, 30])
    end

    it '#first returns the first item' do
      l = Rubyx.eval("[99, 88, 77]")
      expect(l.first.to_ruby).to eq(99)
    end

    it '#count returns the number of items' do
      l = Rubyx.eval("[1, 2, 3, 4]")
      expect(l.count).to eq(4)
    end

    it '#reduce accumulates values' do
      l = Rubyx.eval("[1, 2, 3, 4]")
      sum = l.reduce(0) { |acc, item| acc + item.to_ruby }
      expect(sum).to eq(10)
    end

    it '#any? works' do
      l = Rubyx.eval("[1, 2, 3]")
      expect(l.any? { |item| item.to_ruby == 2 }).to be true
      expect(l.any? { |item| item.to_ruby == 99 }).to be false
    end

    it '#none? works' do
      l = Rubyx.eval("[1, 2, 3]")
      expect(l.none? { |item| item.to_ruby > 10 }).to be true
    end
  end

  # ========== truthy? / falsy? ==========

  describe '#truthy?' do
    it 'returns true for nonzero integer' do
      expect(Rubyx.eval('42').truthy?).to be true
    end

    it 'returns false for zero' do
      expect(Rubyx.eval('0').truthy?).to be false
    end

    it 'returns false for None' do
      expect(Rubyx.eval('None').truthy?).to be false
    end

    it 'returns true for True' do
      expect(Rubyx.eval('True').truthy?).to be true
    end

    it 'returns false for False' do
      expect(Rubyx.eval('False').truthy?).to be false
    end

    it 'returns false for empty string' do
      expect(Rubyx.eval('""').truthy?).to be false
    end

    it 'returns true for nonempty string' do
      expect(Rubyx.eval('"hello"').truthy?).to be true
    end

    it 'returns false for empty list' do
      expect(Rubyx.eval('[]').truthy?).to be false
    end

    it 'returns true for nonempty list' do
      expect(Rubyx.eval('[1]').truthy?).to be true
    end

    it 'returns false for empty dict' do
      expect(Rubyx.eval('{}').truthy?).to be false
    end

    it 'returns true for nonempty dict' do
      expect(Rubyx.eval("{'a': 1}").truthy?).to be true
    end
  end

  describe '#falsy?' do
    it 'is the opposite of truthy?' do
      expect(Rubyx.eval('0').falsy?).to be true
      expect(Rubyx.eval('42').falsy?).to be false
      expect(Rubyx.eval('None').falsy?).to be true
      expect(Rubyx.eval('"hello"').falsy?).to be false
    end
  end

  # ========== callable? ==========

  describe '#callable?' do
    it 'returns true for lambdas' do
      func = Rubyx.eval("lambda x: x * 2")
      expect(func.callable?).to be true
    end

    it 'returns true for user-defined functions' do
      ctx = Rubyx.context
      ctx.eval("def greet(name): return f'Hello {name}'")
      func = ctx.eval("greet")
      expect(func.callable?).to be true
    end

    it 'returns true for classes' do
      ctx = Rubyx.context
      ctx.eval("class Dog:\n    def __init__(self, name):\n        self.name = name")
      cls = ctx.eval("Dog")
      expect(cls.callable?).to be true
    end

    it 'returns true for bound methods' do
      obj = Rubyx.eval("'hello'.upper")
      expect(obj.callable?).to be true
    end

    it 'returns true for instances with __call__' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Multiplier:
            def __init__(self, factor):
                self.factor = factor
            def __call__(self, x):
                return x * self.factor
      PY
      obj = ctx.eval("Multiplier(3)")
      expect(obj.callable?).to be true
    end

    it 'returns true for builtin functions' do
      obj = Rubyx.eval("len")
      expect(obj.callable?).to be true
    end

    it 'returns false for integers' do
      expect(Rubyx.eval('42').callable?).to be false
    end

    it 'returns false for strings' do
      expect(Rubyx.eval('"hello"').callable?).to be false
    end

    it 'returns false for lists' do
      expect(Rubyx.eval('[1, 2]').callable?).to be false
    end

    it 'returns false for None' do
      expect(Rubyx.eval('None').callable?).to be false
    end

    it 'returns false for modules' do
      expect(Rubyx.import('os').callable?).to be false
    end
  end

  # ========== calling callable objects ==========

  describe 'calling callable objects' do
    it 'calls a lambda via __call__' do
      func = Rubyx.eval("lambda x: x * 2")
      result = func.__call__(5)
      expect(result.to_ruby).to eq(10)
    end

    it 'calls a user-defined function via __call__' do
      ctx = Rubyx.context
      ctx.eval("def add(a, b): return a + b")
      func = ctx.eval("add")
      result = func.__call__(3, 4)
      expect(result.to_ruby).to eq(7)
    end

    it 'calls a class as constructor via __call__' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Point:
            def __init__(self, x, y):
                self.x = x
                self.y = y
      PY
      cls = ctx.eval("Point")
      instance = cls.__call__(10, 20)
      expect(instance.x.to_ruby).to eq(10)
      expect(instance.y.to_ruby).to eq(20)
    end

    it 'calls a bound method via __call__' do
      obj = Rubyx.eval("'hello'.upper")
      result = obj.__call__
      expect(result.to_ruby).to eq('HELLO')
    end

    it 'calls an instance with __call__' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Doubler:
            def __call__(self, x):
                return x * 2
      PY
      doubler = ctx.eval("Doubler()")
      result = doubler.__call__(21)
      expect(result.to_ruby).to eq(42)
    end

    it 'calls builtin len via __call__' do
      len_func = Rubyx.eval("len")
      list = Rubyx.eval("[1, 2, 3]")
      result = len_func.__call__(list)
      expect(result.to_ruby).to eq(3)
    end

    it 'calls a lambda with no arguments' do
      func = Rubyx.eval("lambda: 42")
      result = func.__call__
      expect(result.to_ruby).to eq(42)
    end

    it 'calls a function with keyword arguments' do
      ctx = Rubyx.context
      ctx.eval("def greet(name, greeting='Hello'): return f'{greeting}, {name}!'")
      func = ctx.eval("greet")
      result = func.__call__('Alice', greeting: 'Hi')
      expect(result.to_ruby).to eq('Hi, Alice!')
    end

    it 'propagates Python errors from callable' do
      ctx = Rubyx.context
      ctx.eval("def boom(): raise ValueError('kaboom')")
      func = ctx.eval("boom")
      expect { func.__call__ }.to raise_error(StandardError, /kaboom/)
    end

    it 'returns callable result that is itself callable' do
      ctx = Rubyx.context
      ctx.eval("def make_adder(n): return lambda x: x + n")
      make_adder = ctx.eval("make_adder")
      add5 = make_adder.__call__(5)
      expect(add5.callable?).to be true
      expect(add5.__call__(10).to_ruby).to eq(15)
    end
  end

  # ========== call ==========

  describe '#call' do
    it 'calls a lambda with no arguments' do
      func = Rubyx.eval("lambda: 42")
      expect(func.call.to_ruby).to eq(42)
    end

    it 'calls a lambda with arguments' do
      func = Rubyx.eval("lambda x, y: x + y")
      expect(func.call(3, 4).to_ruby).to eq(7)
    end

    it 'calls a user-defined function' do
      ctx = Rubyx.context
      ctx.eval("def double(x): return x * 2")
      func = ctx.eval("double")
      expect(func.call(21).to_ruby).to eq(42)
    end

    it 'calls a class as constructor' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Point:
            def __init__(self, x, y):
                self.x = x
                self.y = y
      PY
      cls = ctx.eval("Point")
      pt = cls.call(10, 20)
      expect(pt.x.to_ruby).to eq(10)
      expect(pt.y.to_ruby).to eq(20)
    end

    it 'calls a bound method' do
      obj = Rubyx.eval("'hello'.upper")
      expect(obj.call.to_ruby).to eq('HELLO')
    end

    it 'calls a callable instance' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Doubler:
            def __call__(self, x):
                return x * 2
      PY
      doubler = ctx.eval("Doubler()")
      expect(doubler.call(5).to_ruby).to eq(10)
    end

    it 'calls builtin len' do
      len_func = Rubyx.eval("len")
      list = Rubyx.eval("[1, 2, 3]")
      expect(len_func.call(list).to_ruby).to eq(3)
    end

    it 'supports keyword arguments' do
      ctx = Rubyx.context
      ctx.eval("def greet(name, greeting='Hello'): return f'{greeting}, {name}!'")
      func = ctx.eval("greet")
      expect(func.call('Alice', greeting: 'Hi').to_ruby).to eq('Hi, Alice!')
    end

    it 'raises TypeError for non-callable objects' do
      obj = Rubyx.eval('42')
      expect { obj.call }.to raise_error(StandardError, /not callable/)
    end

    it 'propagates Python errors' do
      ctx = Rubyx.context
      ctx.eval("def boom(): raise ValueError('kaboom')")
      func = ctx.eval("boom")
      expect { func.call }.to raise_error(StandardError, /kaboom/)
    end

    it 'supports .() shorthand syntax' do
      func = Rubyx.eval("lambda x: x * 3")
      expect(func.(7).to_ruby).to eq(21)
    end

    it 'supports chained calls (function factory)' do
      ctx = Rubyx.context
      ctx.eval("def make_adder(n): return lambda x: x + n")
      make_adder = ctx.eval("make_adder")
      add5 = make_adder.call(5)
      expect(add5.call(10).to_ruby).to eq(15)
    end
  end

  # ========== py_type ==========

  describe '#py_type' do
    it 'returns "int" for integers' do
      expect(Rubyx.eval('42').py_type).to eq('int')
    end

    it 'returns "str" for strings' do
      expect(Rubyx.eval('"hello"').py_type).to eq('str')
    end

    it 'returns "float" for floats' do
      expect(Rubyx.eval('3.14').py_type).to eq('float')
    end

    it 'returns "bool" for booleans' do
      expect(Rubyx.eval('True').py_type).to eq('bool')
    end

    it 'returns "list" for lists' do
      expect(Rubyx.eval('[1, 2]').py_type).to eq('list')
    end

    it 'returns "dict" for dicts' do
      expect(Rubyx.eval('{}').py_type).to eq('dict')
    end

    it 'returns "NoneType" for None' do
      expect(Rubyx.eval('None').py_type).to eq('NoneType')
    end

    it 'returns "module" for modules' do
      expect(Rubyx.import('os').py_type).to eq('module')
    end
  end

  # ========== class identity ==========

  describe 'class identity' do
    it 'is a RubyxObject' do
      obj = Rubyx.eval('42')
      expect(obj).to be_a(RubyxObject)
    end

    it 'eval results are RubyxObject' do
      expect(Rubyx.eval('1 + 1')).to be_a(RubyxObject)
    end

    it 'import results are RubyxObject' do
      expect(Rubyx.import('os')).to be_a(RubyxObject)
    end
  end

  # ========== integration with streaming ==========

  describe 'integration with streaming' do
    it 'to_ruby works on streamed values (they are already Ruby)' do
      gen = Rubyx.eval('iter([1, 2, 3])')
      results = Rubyx.stream(gen).to_a
      # Streamed values are already Ruby primitives
      expect(results).to eq([1, 2, 3])
      expect(results.first).to be_a(Integer)
    end

    it 'to_ruby on eval result matches streamed value' do
      eval_result = Rubyx.eval('42')
      stream_result = Rubyx.stream(Rubyx.eval('iter([42])')).first

      expect(eval_result.to_ruby).to eq(stream_result)
    end
  end

  # ========== Enumerable integration ==========

  describe 'Enumerable' do
    it 'supports map' do
      py_list = Rubyx.eval('[1, 2, 3, 4, 5]')
      result = py_list.map { |item| item.to_ruby * 2 }
      expect(result).to eq([2, 4, 6, 8, 10])
    end

    it 'supports select' do
      py_list = Rubyx.eval('[1, 2, 3, 4, 5, 6]')
      result = py_list.select { |item| item.to_ruby.even? }
      expect(result.map(&:to_ruby)).to eq([2, 4, 6])
    end

    it 'supports find' do
      py_list = Rubyx.eval('[10, 20, 30, 40]')
      result = py_list.find { |item| item.to_ruby > 25 }
      expect(result.to_ruby).to eq(30)
    end

    it 'supports count' do
      py_list = Rubyx.eval('[1, 2, 3, 4, 5]')
      expect(py_list.count).to eq(5)
    end

    it 'supports to_a' do
      py_list = Rubyx.eval('[10, 20, 30]')
      arr = py_list.to_a
      expect(arr.length).to eq(3)
      expect(arr.map(&:to_ruby)).to eq([10, 20, 30])
    end

    it 'supports min_by / max_by' do
      py_list = Rubyx.eval('[3, 1, 4, 1, 5, 9]')
      min = py_list.min_by { |item| item.to_ruby }
      max = py_list.max_by { |item| item.to_ruby }
      expect(min.to_ruby).to eq(1)
      expect(max.to_ruby).to eq(9)
    end

    it 'supports flat_map on nested list' do
      py_list = Rubyx.eval('[[1, 2], [3, 4]]')
      result = py_list.flat_map { |sub| sub.map(&:to_ruby) }
      expect(result).to eq([1, 2, 3, 4])
    end

    it 'supports each_with_index' do
      py_list = Rubyx.eval("['a', 'b', 'c']")
      pairs = []
      py_list.each_with_index { |item, i| pairs << [item.to_ruby, i] }
      expect(pairs).to eq([['a', 0], ['b', 1], ['c', 2]])
    end
  end

  # ========== #inspect vs #to_s ==========

  describe '#inspect vs #to_s' do
    it 'inspect shows repr for strings (with quotes)' do
      obj = Rubyx.eval('"hello"')
      expect(obj.to_s).to eq('hello')
      expect(obj.inspect).to eq("'hello'")
    end

    it 'inspect and to_s are same for integers' do
      obj = Rubyx.eval('42')
      expect(obj.to_s).to eq('42')
      expect(obj.inspect).to eq('42')
    end

    it 'inspect shows repr for lists' do
      obj = Rubyx.eval('[1, 2, 3]')
      expect(obj.inspect).to eq('[1, 2, 3]')
      expect(obj.to_s).to eq('[1, 2, 3]')
    end
  end

  # ========== method_missing with kwargs ==========

  describe '#method_missing with keyword arguments' do
    it 'passes keyword arguments to Python methods' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Greeter:
            def greet(self, name, greeting="Hello"):
                return f"{greeting}, {name}!"
      PY
      greeter = ctx.eval('Greeter()')
      result = greeter.greet('Alice', greeting: 'Hi')
      expect(result.to_ruby).to eq('Hi, Alice!')
    end

    it 'works without kwargs' do
      ctx = Rubyx.context
      ctx.eval(<<~PY)
        class Adder:
            def add(self, a, b):
                return a + b
      PY
      adder = ctx.eval('Adder()')
      result = adder.add(3, 4)
      expect(result.to_ruby).to eq(7)
    end
  end
end
