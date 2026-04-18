require_relative 'spec_helper'

RSpec.describe 'bytes and bytearray support', ruby_integration: true do
  # ========== Python bytes -> Ruby ==========

  describe 'Python bytes to Ruby' do
    it 'converts bytes to Ruby String with ASCII-8BIT encoding' do
      result = Rubyx.eval("b'hello'").to_ruby
      expect(result).to eq("hello".b)
      expect(result).to be_a(String)
      expect(result.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'converts empty bytes' do
      result = Rubyx.eval("b''").to_ruby
      expect(result).to eq("".b)
      expect(result.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'preserves null bytes' do
      result = Rubyx.eval("b'\\x00\\x01\\x02'").to_ruby
      expect(result).to eq("\x00\x01\x02".b)
      expect(result.bytesize).to eq(3)
    end

    it 'preserves high bytes (non-UTF8)' do
      result = Rubyx.eval("b'\\xff\\xfe\\xfd'").to_ruby
      expect(result).to eq("\xff\xfe\xfd".b)
      expect(result.bytesize).to eq(3)
    end

    it 'handles bytes with mixed content' do
      result = Rubyx.eval("b'hello\\x00world'").to_ruby
      expect(result).to eq("hello\x00world".b)
      expect(result.bytesize).to eq(11)
    end
  end

  # ========== Python bytearray -> Ruby ==========

  describe 'Python bytearray to Ruby' do
    it 'converts bytearray to Ruby String with ASCII-8BIT encoding' do
      result = Rubyx.eval("bytearray(b'hello')").to_ruby
      expect(result).to eq("hello".b)
      expect(result).to be_a(String)
      expect(result.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'converts empty bytearray' do
      result = Rubyx.eval("bytearray()").to_ruby
      expect(result).to eq("".b)
      expect(result.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'converts bytearray from list of ints' do
      result = Rubyx.eval("bytearray([0, 127, 255])").to_ruby
      expect(result).to eq("\x00\x7f\xff".b)
      expect(result.bytesize).to eq(3)
    end
  end

  # ========== py_type ==========

  describe '#py_type for bytes/bytearray' do
    it 'returns "bytes" for bytes objects' do
      expect(Rubyx.eval("b'hello'").py_type).to eq('bytes')
    end

    it 'returns "bytearray" for bytearray objects' do
      expect(Rubyx.eval("bytearray(b'hello')").py_type).to eq('bytearray')
    end
  end

  # ========== Ruby binary string -> Python ==========

  describe 'Ruby binary string to Python' do
    it 'passes ASCII-8BIT string as Python bytes via context kwargs' do
      ctx = Rubyx::Context.new
      ctx.eval("def check_type(x): return type(x).__name__")
      result = ctx.eval('check_type(data)', data: "hello".b)
      expect(result.to_ruby).to eq('bytes')
    end

    it 'passes UTF-8 string as Python str via context kwargs' do
      ctx = Rubyx::Context.new
      ctx.eval("def check_type(x): return type(x).__name__")
      result = ctx.eval('check_type(data)', data: "hello")
      expect(result.to_ruby).to eq('str')
    end

    it 'roundtrips binary data through Python' do
      binary = "\x00\x01\xff\xfe\x80".b
      ctx = Rubyx::Context.new
      result = ctx.eval('data', data: binary)
      ruby_result = result.to_ruby
      expect(ruby_result).to eq(binary)
      expect(ruby_result.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'roundtrips empty binary string' do
      binary = "".b
      ctx = Rubyx::Context.new
      result = ctx.eval('data', data: binary)
      ruby_result = result.to_ruby
      expect(ruby_result).to eq(binary)
      expect(ruby_result.encoding).to eq(Encoding::ASCII_8BIT)
    end
  end

  # ========== Streaming bytes ==========

  describe 'streaming bytes' do
    it 'streams bytes values from a generator' do
      gen = Rubyx.eval("iter([b'aaa', b'bbb', b'ccc'])")
      results = Rubyx.stream(gen).to_a
      expect(results).to all(be_a(String))
      expect(results.map(&:encoding)).to all(eq(Encoding::ASCII_8BIT))
      expect(results).to eq(["aaa".b, "bbb".b, "ccc".b])
    end

    it 'streams bytearray values from a generator' do
      gen = Rubyx.eval("iter([bytearray(b'x'), bytearray(b'y')])")
      results = Rubyx.stream(gen).to_a
      expect(results).to all(be_a(String))
      expect(results.map(&:encoding)).to all(eq(Encoding::ASCII_8BIT))
      expect(results).to eq(["x".b, "y".b])
    end

    it 'streams mixed types including bytes' do
      gen = Rubyx.eval("iter([1, b'hello', 'world', 3.14])")
      results = Rubyx.stream(gen).to_a
      expect(results[0]).to eq(1)
      expect(results[1]).to eq("hello".b)
      expect(results[1].encoding).to eq(Encoding::ASCII_8BIT)
      expect(results[2]).to eq('world')
      expect(results[3]).to be_within(0.001).of(3.14)
    end
  end

  # ========== Bytes in nested structures ==========

  describe 'bytes in nested structures' do
    it 'converts bytes inside a list' do
      result = Rubyx.eval("[b'a', b'b', b'c']").to_ruby
      expect(result).to eq(["a".b, "b".b, "c".b])
      expect(result).to all(be_a(String))
      result.each { |s| expect(s.encoding).to eq(Encoding::ASCII_8BIT) }
    end

    it 'converts bytes inside a dict' do
      result = Rubyx.eval("{'key': b'value'}").to_ruby
      expect(result['key']).to eq("value".b)
      expect(result['key'].encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'converts bytes as dict keys' do
      result = Rubyx.eval("{b'key': 42}").to_ruby
      expect(result["key".b]).to eq(42)
    end
  end

  # ========== Python stdlib integration ==========

  describe 'Python stdlib with bytes' do
    it 'works with base64 encode/decode' do
      ctx = Rubyx::Context.new
      ctx.eval("import base64")
      encoded = ctx.eval("base64.b64encode(data)", data: "Hello, World!".b)
      expect(encoded.to_ruby).to eq("SGVsbG8sIFdvcmxkIQ==".b)
      expect(encoded.to_ruby.encoding).to eq(Encoding::ASCII_8BIT)
    end

    it 'works with hashlib' do
      ctx = Rubyx::Context.new
      ctx.eval("import hashlib")
      digest = ctx.eval("hashlib.sha256(data).hexdigest()", data: "test".b)
      expect(digest.to_ruby).to eq('9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08')
    end
  end
end
