/// <reference types="vitest/globals" />
/// <reference types="@testing-library/jest-dom" />

declare module 'vitest' {
  interface Assertion<T> {
    toHaveNoViolations(): void;
  }
  interface AsymmetricMatchersContaining {
    toHaveNoViolations(): void;
  }
}
