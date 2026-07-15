package net.knego.hiperf.smrcollections;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import net.knego.hiperf.common.SmrConfig;
import org.junit.jupiter.api.Test;

class GoldenTest {
    @Test
    void crossLanguageGoldenBytes() throws Exception {
        // Gradle test working dir is the subproject dir (java/smr-collections-common).
        Path golden = Path.of("..", "..", "rust", "smr-collections", "testdata", "golden_snapshot.bin");
        byte[] want = Files.readAllBytes(golden);
        SmrConfig c = new SmrConfig(4096, 64, 1, 0, 2000, 0, 0);
        Book b = new Book(c);
        Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
        Workload.Insert ins = new Workload.Insert();
        for (int i = 0; i < c.steady(); i++) {
            Workload.nextInsert(rng, i, c.levels(), c.tick(), c.priceMin(), ins);
            b.insert(ins.orderId, ins.price, ins.qty, ins.side);
        }
        Snapshotter s = new Snapshotter(4 * 1024 * 1024);
        int len = s.encode(b);
        byte[] got = Arrays.copyOf(s.backing(), len);
        assertArrayEquals(want, got, "java snapshot bytes must equal the rust golden");
    }
}
