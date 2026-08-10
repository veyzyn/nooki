import { describe, expect, it } from 'vitest';
import { chartDomainMax } from './ui';

describe('chartDomainMax', () => {
  it('keeps low activity readable instead of forcing a 100 percent scale', () => {
    expect(chartDomainMax([0, 2, 6], 10)).toBe(10);
    expect(chartDomainMax([12, 16], 25)).toBe(25);
  });

  it('adds headroom for larger spikes', () => {
    expect(chartDomainMax([18, 26, 41], 10)).toBe(50);
    expect(chartDomainMax([72, 91], 10)).toBe(125);
  });
});
