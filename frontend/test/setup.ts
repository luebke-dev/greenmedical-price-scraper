// Runs before each spec file.
import { config } from '@vue/test-utils';

// Quasar's QTooltip/QMenu portals and transitions are irrelevant for unit tests.
config.global.stubs = {
  ...config.global.stubs,
  transition: false,
  'transition-group': false,
};

export {};
