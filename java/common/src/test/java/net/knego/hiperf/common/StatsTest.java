package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class StatsTest {

    @Test
    void percentileSingleElement() {
        long[] s = {42};
        assertEquals(42, Stats.percentile(s, 50));
        assertEquals(42, Stats.percentile(s, 99));
        assertEquals(42, Stats.percentile(s, 0));
        assertEquals(42, Stats.percentile(s, 100));
    }

    @Test
    void percentileNearestRankNoInterpolation() {
        // n = 10, indices 0..9. Values 10,20,...,100.
        long[] s = {10, 20, 30, 40, 50, 60, 70, 80, 90, 100};
        // p50 -> floor(0.50 * 9) = floor(4.5) = 4 -> value 50
        assertEquals(50, Stats.percentile(s, 50));
        // p99 -> floor(0.99 * 9) = floor(8.91) = 8 -> value 90
        assertEquals(90, Stats.percentile(s, 99));
        // p0 -> index 0 -> 10
        assertEquals(10, Stats.percentile(s, 0));
        // p100 -> floor(1.0 * 9) = 9 -> 100
        assertEquals(100, Stats.percentile(s, 100));
    }

    @Test
    void percentileLargeArrayIndices() {
        // n = 100000 -> p50 index = floor(0.50 * 99999) = 49999
        //              p99 index = floor(0.99 * 99999) = 98999
        int n = 100000;
        long[] s = new long[n];
        for (int i = 0; i < n; i++) {
            s[i] = i; // sorted ascending, value == index
        }
        assertEquals(49999, Stats.percentile(s, 50));
        assertEquals(98999, Stats.percentile(s, 99));
    }

    @Test
    void percentileOddSize() {
        // n = 5, indices 0..4. Values 1,2,3,4,5.
        long[] s = {1, 2, 3, 4, 5};
        // p50 -> floor(0.50 * 4) = 2 -> value 3
        assertEquals(3, Stats.percentile(s, 50));
        // p99 -> floor(0.99 * 4) = floor(3.96) = 3 -> value 4
        assertEquals(4, Stats.percentile(s, 99));
    }

    @Test
    void meanSimple() {
        long[] s = {10, 20, 30, 40};
        assertEquals(25.0, Stats.mean(s), 1e-9);
    }

    @Test
    void meanSingleElement() {
        long[] s = {7};
        assertEquals(7.0, Stats.mean(s), 1e-9);
    }

    @Test
    void meanFractional() {
        long[] s = {1, 2, 3};
        assertEquals(2.0, Stats.mean(s), 1e-9);
        long[] s2 = {1, 2};
        assertEquals(1.5, Stats.mean(s2), 1e-9);
    }
}
