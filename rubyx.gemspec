require_relative 'lib/rubyx/version'

Gem::Specification.new do |spec|
  spec.name = "rubyx-py"
  spec.version = Rubyx::VERSION
  spec.license = 'MIT'
  spec.authors = ["Naiker"]
  spec.email = ["yinho999@gmail.com"]

  spec.summary = "Ruby-Python bridge powered by Rust"
  spec.description = "Call Python libraries directly from Ruby and Rails. " \
                     "No microservices, no REST APIs — just seamless interop. " \
                     "Powered by Rust for safety and performance."
  spec.homepage = "https://github.com/yinho999/rubyx"

  spec.metadata = {
    "homepage_uri" => spec.homepage,
    "source_code_uri" => spec.homepage,
    "changelog_uri" => "#{spec.homepage}/blob/main/CHANGELOG.md",
    "bug_tracker_uri" => "#{spec.homepage}/issues",
    "rubygems_mfa_required" => "true",
  }

  spec.required_ruby_version = Gem::Requirement.new(">= 3.0.0")

  spec.files = Dir["lib/**/*.{rb,rake,toml,py}", "ext/**/*.{rb,rs,toml,py}", "Cargo.toml", "LICENSE", "README.md", "docs/assets/logo.png"]
  spec.require_paths = ["lib"]
  spec.extensions = ['ext/rubyx/extconf.rb']

  spec.add_dependency 'rb_sys', '~> 0.9'
  spec.add_development_dependency 'rake-compiler', '~> 1.2'
  spec.add_development_dependency 'rspec', '~> 3.0'
end