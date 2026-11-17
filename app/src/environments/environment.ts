import { isDevMode } from '@angular/core';

export const environment = {
  /** Prefilled in the register form: the dev server, nothing in a release (no official instance). */
  defaultInstance: isDevMode() ? 'http://localhost:8080' : '',
};
