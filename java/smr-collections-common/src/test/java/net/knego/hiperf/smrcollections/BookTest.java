package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertEquals;

import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class BookTest {
    private static SmrConfig cfg() {
        return new SmrConfig(1024, 16, 1, 0, 100, 0, 0, 4096, 200000, 20000, 100);
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

    private static SmrConfig churnCfg() {
        return new SmrConfig(1024, 16, 1L, 0L, 100, 0, 0, 256, 200_000, 20_000, 100);
    }

    @Test
    void cancelUnlinksMiddleOfLevelFifo() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.insert(2, 5, 7, (byte) 0);
        b.insert(3, 5, 3, (byte) 0);
        b.cancel(2);
        assertEquals(13, b.levelQty((byte) 0, 5), "middle order's qty leaves the level");
        assertEquals(2, b.bids[5].count);
        assertEquals(0, b.bids[5].head, "head unchanged");
        assertEquals(2, b.bids[5].tail, "tail unchanged");
        assertEquals(2, b.pool[0].next, "head now links past the cancelled slot");
        assertEquals(0, b.pool[2].prev);
    }

    @Test
    void cancelHeadAndTailFixLevelEnds() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.insert(2, 5, 7, (byte) 0);
        b.cancel(1); // head
        assertEquals(1, b.bids[5].head, "head advances to the survivor");
        assertEquals(Book.NIL, b.pool[1].prev);
        b.cancel(2); // tail; level now empty
        assertEquals(Book.NIL, b.bids[5].head);
        assertEquals(Book.NIL, b.bids[5].tail);
        assertEquals(0, b.bids[5].count);
        assertEquals(0, b.levelQty((byte) 0, 5));
    }

    @Test
    void cancelEmptyingBestLevelRescans() {
        Book b = new Book(churnCfg());
        b.insert(1, 3, 10, (byte) 0);
        b.insert(2, 9, 10, (byte) 0); // best bid = 9
        b.insert(3, 4, 10, (byte) 1);
        b.insert(4, 2, 10, (byte) 1); // best ask = 2
        b.cancel(2);
        assertEquals(3, b.bestBid(), "best bid falls back to the next occupied below");
        b.cancel(4);
        assertEquals(4, b.bestAsk(), "best ask rises to the next occupied above");
        b.cancel(1);
        assertEquals(-1, b.bestBid(), "no bids left");
    }

    @Test
    void cancelledSlotsAreReusedLifo() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0); // slot 0
        b.insert(2, 5, 10, (byte) 0); // slot 1
        b.insert(3, 5, 10, (byte) 0); // slot 2
        b.cancel(1); // free: 0
        b.cancel(3); // free: 2 -> 0
        assertEquals(2, b.freeHead);
        b.insert(4, 5, 10, (byte) 0);
        assertEquals(2, b.getSlot(4), "LIFO: most recently freed slot first");
        b.insert(5, 5, 10, (byte) 0);
        assertEquals(0, b.getSlot(5));
        b.insert(6, 5, 10, (byte) 0);
        assertEquals(3, b.getSlot(6), "free list empty -> bump hwm");
        assertEquals(4, b.hwm());
    }

    @Test
    void freedSlotIsMarkedWithZeroOrderId() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.cancel(1);
        assertEquals(0, b.pool[0].orderId, "freed marker for the snapshot walk");
    }

    @Test
    void fillCompletesThenFreesTheSlot() {
        Book b = new Book(churnCfg());
        b.insert(1, 5, 10, (byte) 0);
        b.update(1, 4); // partial: remaining 6
        assertEquals(6, b.levelQty((byte) 0, 5));
        b.fill(1);
        assertEquals(0, b.levelQty((byte) 0, 5), "remaining 6 leaves the level");
        assertEquals(0, b.bids[5].count);
        assertEquals(0, b.freeHead, "slot recycled like a cancel");
    }
}
