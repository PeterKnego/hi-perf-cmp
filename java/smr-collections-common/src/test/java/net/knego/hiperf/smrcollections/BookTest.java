package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertEquals;

import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class BookTest {
    private static SmrConfig cfg() {
        return new SmrConfig(1024, 16, 1, 0, 100, 0, 0);
    }

    @Test
    void insertPlacesOrder() {
        Book b = new Book(cfg());
        b.insert(1, 5, 10, (byte) 0);
        b.insert(2, 5, 7, (byte) 0);
        b.insert(3, 8, 3, (byte) 1);
        assertEquals(17, b.levelQty((byte) 0, 5));
        assertEquals(3, b.levelQty((byte) 1, 8));
        assertEquals(5, b.bestBid());
        assertEquals(8, b.bestAsk());
        assertEquals(1, b.getSlot(2));
    }

    @Test
    void updateCapsFill() {
        Book b = new Book(cfg());
        b.insert(1, 5, 10, (byte) 0);
        b.update(1, 4);
        assertEquals(6, b.levelQty((byte) 0, 5));
        b.update(1, 100);
        assertEquals(0, b.levelQty((byte) 0, 5));
    }
}
