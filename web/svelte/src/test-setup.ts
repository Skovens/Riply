import * as matchers from '@testing-library/jest-dom/matchers';
import { expect, vi } from 'vitest';

vi.useFakeTimers();

expect.extend(matchers);
