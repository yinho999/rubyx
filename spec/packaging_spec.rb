# frozen_string_literal: true

require 'rubygems/package'

RSpec.describe 'gem packaging' do
  it 'includes the rake tasks file' do
    spec = Gem::Specification.load('rubyx.gemspec')
    expect(spec.files).to include('lib/rubyx/tasks/rubyx.rake')
  end

  it 'includes the railtie' do
    spec = Gem::Specification.load('rubyx.gemspec')
    expect(spec.files).to include('lib/rubyx/railtie.rb')
  end

  it 'includes the install generator templates' do
    spec = Gem::Specification.load('rubyx.gemspec')
    expect(spec.files).to include('lib/generators/rubyx/templates/rubyx_initializer.rb')
    expect(spec.files).to include('lib/generators/rubyx/templates/pyproject.toml')
  end
end