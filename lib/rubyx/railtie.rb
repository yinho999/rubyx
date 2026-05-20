require_relative 'rails'

module Rubyx
  class Railtie < ::Rails::Railtie
    config.rubyx = ActiveSupport::OrderedOptions.new

    # Auto-initialize Python after all config initializers have run
    config.after_initialize do
      if Rubyx::Rails.configuration.auto_init
        Rubyx::Rails.init!
        ::Rails.logger.info '[Rubyx] Python environment initialized successfully'
      end
    end

    # Register rake tasks
    rake_tasks do
      task_file = File.expand_path('tasks/rubyx.rake', __dir__)
      load task_file if File.exist?(task_file)
    end
  end
end
