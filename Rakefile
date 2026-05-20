require 'bundler/gem_tasks'
require 'rb_sys/extensiontask'
require 'rspec/core/rake_task'
RSpec::Core::RakeTask.new(:spec)

GEMSPEC = Gem::Specification.load('rubyx.gemspec')

RbSys::ExtensionTask.new('rubyx', GEMSPEC) do |ext|
  ext.lib_dir = 'lib/rubyx'
end

task default: [:compile, :spec]