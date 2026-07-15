/* Generated SBE (Simple Binary Encoding) message codec. */
package booksnap;

import org.agrona.MutableDirectBuffer;

@SuppressWarnings("all")
public final class BookSnapshotEncoder
{
    public static final int BLOCK_LENGTH = 36;
    public static final int TEMPLATE_ID = 1;
    public static final int SCHEMA_ID = 8;
    public static final int SCHEMA_VERSION = 1;
    public static final String SEMANTIC_VERSION = "";
    public static final java.nio.ByteOrder BYTE_ORDER = java.nio.ByteOrder.LITTLE_ENDIAN;

    private final BookSnapshotEncoder parentMessage = this;
    private MutableDirectBuffer buffer;
    private int offset;
    private int limit;

    public int sbeBlockLength()
    {
        return BLOCK_LENGTH;
    }

    public int sbeTemplateId()
    {
        return TEMPLATE_ID;
    }

    public int sbeSchemaId()
    {
        return SCHEMA_ID;
    }

    public int sbeSchemaVersion()
    {
        return SCHEMA_VERSION;
    }

    public String sbeSemanticType()
    {
        return "";
    }

    public MutableDirectBuffer buffer()
    {
        return buffer;
    }

    public int offset()
    {
        return offset;
    }

    public BookSnapshotEncoder wrap(final MutableDirectBuffer buffer, final int offset)
    {
        if (buffer != this.buffer)
        {
            this.buffer = buffer;
        }
        this.offset = offset;
        limit(offset + BLOCK_LENGTH);

        return this;
    }

    public BookSnapshotEncoder wrapAndApplyHeader(
        final MutableDirectBuffer buffer, final int offset, final MessageHeaderEncoder headerEncoder)
    {
        headerEncoder
            .wrap(buffer, offset)
            .blockLength(BLOCK_LENGTH)
            .templateId(TEMPLATE_ID)
            .schemaId(SCHEMA_ID)
            .version(SCHEMA_VERSION);

        return wrap(buffer, offset + MessageHeaderEncoder.ENCODED_LENGTH);
    }

    public int encodedLength()
    {
        return limit - offset;
    }

    public int limit()
    {
        return limit;
    }

    public void limit(final int limit)
    {
        this.limit = limit;
    }

    public static int priceMinId()
    {
        return 1;
    }

    public static int priceMinSinceVersion()
    {
        return 0;
    }

    public static int priceMinEncodingOffset()
    {
        return 0;
    }

    public static int priceMinEncodingLength()
    {
        return 8;
    }

    public static String priceMinMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static long priceMinNullValue()
    {
        return -9223372036854775808L;
    }

    public static long priceMinMinValue()
    {
        return -9223372036854775807L;
    }

    public static long priceMinMaxValue()
    {
        return 9223372036854775807L;
    }

    public BookSnapshotEncoder priceMin(final long value)
    {
        buffer.putLong(offset + 0, value, BYTE_ORDER);
        return this;
    }


    public static int tickSizeId()
    {
        return 2;
    }

    public static int tickSizeSinceVersion()
    {
        return 0;
    }

    public static int tickSizeEncodingOffset()
    {
        return 8;
    }

    public static int tickSizeEncodingLength()
    {
        return 8;
    }

    public static String tickSizeMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static long tickSizeNullValue()
    {
        return -9223372036854775808L;
    }

    public static long tickSizeMinValue()
    {
        return -9223372036854775807L;
    }

    public static long tickSizeMaxValue()
    {
        return 9223372036854775807L;
    }

    public BookSnapshotEncoder tickSize(final long value)
    {
        buffer.putLong(offset + 8, value, BYTE_ORDER);
        return this;
    }


    public static int nLevelsId()
    {
        return 3;
    }

    public static int nLevelsSinceVersion()
    {
        return 0;
    }

    public static int nLevelsEncodingOffset()
    {
        return 16;
    }

    public static int nLevelsEncodingLength()
    {
        return 4;
    }

    public static String nLevelsMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static long nLevelsNullValue()
    {
        return 4294967295L;
    }

    public static long nLevelsMinValue()
    {
        return 0L;
    }

    public static long nLevelsMaxValue()
    {
        return 4294967294L;
    }

    public BookSnapshotEncoder nLevels(final long value)
    {
        buffer.putInt(offset + 16, (int)value, BYTE_ORDER);
        return this;
    }


    public static int capacityId()
    {
        return 4;
    }

    public static int capacitySinceVersion()
    {
        return 0;
    }

    public static int capacityEncodingOffset()
    {
        return 20;
    }

    public static int capacityEncodingLength()
    {
        return 4;
    }

    public static String capacityMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static long capacityNullValue()
    {
        return 4294967295L;
    }

    public static long capacityMinValue()
    {
        return 0L;
    }

    public static long capacityMaxValue()
    {
        return 4294967294L;
    }

    public BookSnapshotEncoder capacity(final long value)
    {
        buffer.putInt(offset + 20, (int)value, BYTE_ORDER);
        return this;
    }


    public static int hwmId()
    {
        return 5;
    }

    public static int hwmSinceVersion()
    {
        return 0;
    }

    public static int hwmEncodingOffset()
    {
        return 24;
    }

    public static int hwmEncodingLength()
    {
        return 4;
    }

    public static String hwmMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static long hwmNullValue()
    {
        return 4294967295L;
    }

    public static long hwmMinValue()
    {
        return 0L;
    }

    public static long hwmMaxValue()
    {
        return 4294967294L;
    }

    public BookSnapshotEncoder hwm(final long value)
    {
        buffer.putInt(offset + 24, (int)value, BYTE_ORDER);
        return this;
    }


    public static int bestBidId()
    {
        return 6;
    }

    public static int bestBidSinceVersion()
    {
        return 0;
    }

    public static int bestBidEncodingOffset()
    {
        return 28;
    }

    public static int bestBidEncodingLength()
    {
        return 4;
    }

    public static String bestBidMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static int bestBidNullValue()
    {
        return -2147483648;
    }

    public static int bestBidMinValue()
    {
        return -2147483647;
    }

    public static int bestBidMaxValue()
    {
        return 2147483647;
    }

    public BookSnapshotEncoder bestBid(final int value)
    {
        buffer.putInt(offset + 28, value, BYTE_ORDER);
        return this;
    }


    public static int bestAskId()
    {
        return 7;
    }

    public static int bestAskSinceVersion()
    {
        return 0;
    }

    public static int bestAskEncodingOffset()
    {
        return 32;
    }

    public static int bestAskEncodingLength()
    {
        return 4;
    }

    public static String bestAskMetaAttribute(final MetaAttribute metaAttribute)
    {
        if (MetaAttribute.PRESENCE == metaAttribute)
        {
            return "required";
        }

        return "";
    }

    public static int bestAskNullValue()
    {
        return -2147483648;
    }

    public static int bestAskMinValue()
    {
        return -2147483647;
    }

    public static int bestAskMaxValue()
    {
        return 2147483647;
    }

    public BookSnapshotEncoder bestAsk(final int value)
    {
        buffer.putInt(offset + 32, value, BYTE_ORDER);
        return this;
    }


    private final LevelsEncoder levels = new LevelsEncoder(this);

    public static long levelsId()
    {
        return 10;
    }

    public LevelsEncoder levelsCount(final int count)
    {
        levels.wrap(buffer, count);
        return levels;
    }

    public static final class LevelsEncoder
    {
        public static final int HEADER_SIZE = 4;
        private final BookSnapshotEncoder parentMessage;
        private MutableDirectBuffer buffer;
        private int count;
        private int index;
        private int offset;
        private int initialLimit;

        LevelsEncoder(final BookSnapshotEncoder parentMessage)
        {
            this.parentMessage = parentMessage;
        }

        public void wrap(final MutableDirectBuffer buffer, final int count)
        {
            if (count < 0 || count > 65534)
            {
                throw new IllegalArgumentException("count outside allowed range: count=" + count);
            }

            if (buffer != this.buffer)
            {
                this.buffer = buffer;
            }

            index = 0;
            this.count = count;
            final int limit = parentMessage.limit();
            initialLimit = limit;
            parentMessage.limit(limit + HEADER_SIZE);
            buffer.putShort(limit + 0, (short)25, BYTE_ORDER);
            buffer.putShort(limit + 2, (short)count, BYTE_ORDER);
        }

        public LevelsEncoder next()
        {
            if (index >= count)
            {
                throw new java.util.NoSuchElementException();
            }

            offset = parentMessage.limit();
            parentMessage.limit(offset + sbeBlockLength());
            ++index;

            return this;
        }

        public int resetCountToIndex()
        {
            count = index;
            buffer.putShort(initialLimit + 2, (short)count, BYTE_ORDER);

            return count;
        }

        public static int countMinValue()
        {
            return 0;
        }

        public static int countMaxValue()
        {
            return 65534;
        }

        public static int sbeHeaderSize()
        {
            return HEADER_SIZE;
        }

        public static int sbeBlockLength()
        {
            return 25;
        }

        public static int sideId()
        {
            return 11;
        }

        public static int sideSinceVersion()
        {
            return 0;
        }

        public static int sideEncodingOffset()
        {
            return 0;
        }

        public static int sideEncodingLength()
        {
            return 1;
        }

        public static String sideMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public LevelsEncoder side(final Side value)
        {
            buffer.putByte(offset + 0, (byte)value.value());
            return this;
        }

        public static int levelTickId()
        {
            return 12;
        }

        public static int levelTickSinceVersion()
        {
            return 0;
        }

        public static int levelTickEncodingOffset()
        {
            return 1;
        }

        public static int levelTickEncodingLength()
        {
            return 4;
        }

        public static String levelTickMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long levelTickNullValue()
        {
            return 4294967295L;
        }

        public static long levelTickMinValue()
        {
            return 0L;
        }

        public static long levelTickMaxValue()
        {
            return 4294967294L;
        }

        public LevelsEncoder levelTick(final long value)
        {
            buffer.putInt(offset + 1, (int)value, BYTE_ORDER);
            return this;
        }


        public static int qtyTotalId()
        {
            return 13;
        }

        public static int qtyTotalSinceVersion()
        {
            return 0;
        }

        public static int qtyTotalEncodingOffset()
        {
            return 5;
        }

        public static int qtyTotalEncodingLength()
        {
            return 8;
        }

        public static String qtyTotalMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long qtyTotalNullValue()
        {
            return -9223372036854775808L;
        }

        public static long qtyTotalMinValue()
        {
            return -9223372036854775807L;
        }

        public static long qtyTotalMaxValue()
        {
            return 9223372036854775807L;
        }

        public LevelsEncoder qtyTotal(final long value)
        {
            buffer.putLong(offset + 5, value, BYTE_ORDER);
            return this;
        }


        public static int orderCountId()
        {
            return 14;
        }

        public static int orderCountSinceVersion()
        {
            return 0;
        }

        public static int orderCountEncodingOffset()
        {
            return 13;
        }

        public static int orderCountEncodingLength()
        {
            return 4;
        }

        public static String orderCountMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long orderCountNullValue()
        {
            return 4294967295L;
        }

        public static long orderCountMinValue()
        {
            return 0L;
        }

        public static long orderCountMaxValue()
        {
            return 4294967294L;
        }

        public LevelsEncoder orderCount(final long value)
        {
            buffer.putInt(offset + 13, (int)value, BYTE_ORDER);
            return this;
        }


        public static int headId()
        {
            return 15;
        }

        public static int headSinceVersion()
        {
            return 0;
        }

        public static int headEncodingOffset()
        {
            return 17;
        }

        public static int headEncodingLength()
        {
            return 4;
        }

        public static String headMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long headNullValue()
        {
            return 4294967295L;
        }

        public static long headMinValue()
        {
            return 0L;
        }

        public static long headMaxValue()
        {
            return 4294967294L;
        }

        public LevelsEncoder head(final long value)
        {
            buffer.putInt(offset + 17, (int)value, BYTE_ORDER);
            return this;
        }


        public static int tailId()
        {
            return 16;
        }

        public static int tailSinceVersion()
        {
            return 0;
        }

        public static int tailEncodingOffset()
        {
            return 21;
        }

        public static int tailEncodingLength()
        {
            return 4;
        }

        public static String tailMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long tailNullValue()
        {
            return 4294967295L;
        }

        public static long tailMinValue()
        {
            return 0L;
        }

        public static long tailMaxValue()
        {
            return 4294967294L;
        }

        public LevelsEncoder tail(final long value)
        {
            buffer.putInt(offset + 21, (int)value, BYTE_ORDER);
            return this;
        }

    }

    private final OrdersEncoder orders = new OrdersEncoder(this);

    public static long ordersId()
    {
        return 20;
    }

    public OrdersEncoder ordersCount(final int count)
    {
        orders.wrap(buffer, count);
        return orders;
    }

    public static final class OrdersEncoder
    {
        public static final int HEADER_SIZE = 4;
        private final BookSnapshotEncoder parentMessage;
        private MutableDirectBuffer buffer;
        private int count;
        private int index;
        private int offset;
        private int initialLimit;

        OrdersEncoder(final BookSnapshotEncoder parentMessage)
        {
            this.parentMessage = parentMessage;
        }

        public void wrap(final MutableDirectBuffer buffer, final int count)
        {
            if (count < 0 || count > 65534)
            {
                throw new IllegalArgumentException("count outside allowed range: count=" + count);
            }

            if (buffer != this.buffer)
            {
                this.buffer = buffer;
            }

            index = 0;
            this.count = count;
            final int limit = parentMessage.limit();
            initialLimit = limit;
            parentMessage.limit(limit + HEADER_SIZE);
            buffer.putShort(limit + 0, (short)45, BYTE_ORDER);
            buffer.putShort(limit + 2, (short)count, BYTE_ORDER);
        }

        public OrdersEncoder next()
        {
            if (index >= count)
            {
                throw new java.util.NoSuchElementException();
            }

            offset = parentMessage.limit();
            parentMessage.limit(offset + sbeBlockLength());
            ++index;

            return this;
        }

        public int resetCountToIndex()
        {
            count = index;
            buffer.putShort(initialLimit + 2, (short)count, BYTE_ORDER);

            return count;
        }

        public static int countMinValue()
        {
            return 0;
        }

        public static int countMaxValue()
        {
            return 65534;
        }

        public static int sbeHeaderSize()
        {
            return HEADER_SIZE;
        }

        public static int sbeBlockLength()
        {
            return 45;
        }

        public static int slotId()
        {
            return 21;
        }

        public static int slotSinceVersion()
        {
            return 0;
        }

        public static int slotEncodingOffset()
        {
            return 0;
        }

        public static int slotEncodingLength()
        {
            return 4;
        }

        public static String slotMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long slotNullValue()
        {
            return 4294967295L;
        }

        public static long slotMinValue()
        {
            return 0L;
        }

        public static long slotMaxValue()
        {
            return 4294967294L;
        }

        public OrdersEncoder slot(final long value)
        {
            buffer.putInt(offset + 0, (int)value, BYTE_ORDER);
            return this;
        }


        public static int orderIdId()
        {
            return 22;
        }

        public static int orderIdSinceVersion()
        {
            return 0;
        }

        public static int orderIdEncodingOffset()
        {
            return 4;
        }

        public static int orderIdEncodingLength()
        {
            return 8;
        }

        public static String orderIdMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long orderIdNullValue()
        {
            return -9223372036854775808L;
        }

        public static long orderIdMinValue()
        {
            return -9223372036854775807L;
        }

        public static long orderIdMaxValue()
        {
            return 9223372036854775807L;
        }

        public OrdersEncoder orderId(final long value)
        {
            buffer.putLong(offset + 4, value, BYTE_ORDER);
            return this;
        }


        public static int priceId()
        {
            return 23;
        }

        public static int priceSinceVersion()
        {
            return 0;
        }

        public static int priceEncodingOffset()
        {
            return 12;
        }

        public static int priceEncodingLength()
        {
            return 8;
        }

        public static String priceMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long priceNullValue()
        {
            return -9223372036854775808L;
        }

        public static long priceMinValue()
        {
            return -9223372036854775807L;
        }

        public static long priceMaxValue()
        {
            return 9223372036854775807L;
        }

        public OrdersEncoder price(final long value)
        {
            buffer.putLong(offset + 12, value, BYTE_ORDER);
            return this;
        }


        public static int qtyId()
        {
            return 24;
        }

        public static int qtySinceVersion()
        {
            return 0;
        }

        public static int qtyEncodingOffset()
        {
            return 20;
        }

        public static int qtyEncodingLength()
        {
            return 8;
        }

        public static String qtyMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long qtyNullValue()
        {
            return -9223372036854775808L;
        }

        public static long qtyMinValue()
        {
            return -9223372036854775807L;
        }

        public static long qtyMaxValue()
        {
            return 9223372036854775807L;
        }

        public OrdersEncoder qty(final long value)
        {
            buffer.putLong(offset + 20, value, BYTE_ORDER);
            return this;
        }


        public static int filledId()
        {
            return 25;
        }

        public static int filledSinceVersion()
        {
            return 0;
        }

        public static int filledEncodingOffset()
        {
            return 28;
        }

        public static int filledEncodingLength()
        {
            return 8;
        }

        public static String filledMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long filledNullValue()
        {
            return -9223372036854775808L;
        }

        public static long filledMinValue()
        {
            return -9223372036854775807L;
        }

        public static long filledMaxValue()
        {
            return 9223372036854775807L;
        }

        public OrdersEncoder filled(final long value)
        {
            buffer.putLong(offset + 28, value, BYTE_ORDER);
            return this;
        }


        public static int sideId()
        {
            return 26;
        }

        public static int sideSinceVersion()
        {
            return 0;
        }

        public static int sideEncodingOffset()
        {
            return 36;
        }

        public static int sideEncodingLength()
        {
            return 1;
        }

        public static String sideMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public OrdersEncoder side(final Side value)
        {
            buffer.putByte(offset + 36, (byte)value.value());
            return this;
        }

        public static int nextSlotId()
        {
            return 27;
        }

        public static int nextSlotSinceVersion()
        {
            return 0;
        }

        public static int nextSlotEncodingOffset()
        {
            return 37;
        }

        public static int nextSlotEncodingLength()
        {
            return 4;
        }

        public static String nextSlotMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long nextSlotNullValue()
        {
            return 4294967295L;
        }

        public static long nextSlotMinValue()
        {
            return 0L;
        }

        public static long nextSlotMaxValue()
        {
            return 4294967294L;
        }

        public OrdersEncoder nextSlot(final long value)
        {
            buffer.putInt(offset + 37, (int)value, BYTE_ORDER);
            return this;
        }


        public static int prevId()
        {
            return 28;
        }

        public static int prevSinceVersion()
        {
            return 0;
        }

        public static int prevEncodingOffset()
        {
            return 41;
        }

        public static int prevEncodingLength()
        {
            return 4;
        }

        public static String prevMetaAttribute(final MetaAttribute metaAttribute)
        {
            if (MetaAttribute.PRESENCE == metaAttribute)
            {
                return "required";
            }

            return "";
        }

        public static long prevNullValue()
        {
            return 4294967295L;
        }

        public static long prevMinValue()
        {
            return 0L;
        }

        public static long prevMaxValue()
        {
            return 4294967294L;
        }

        public OrdersEncoder prev(final long value)
        {
            buffer.putInt(offset + 41, (int)value, BYTE_ORDER);
            return this;
        }

    }

    public String toString()
    {
        if (null == buffer)
        {
            return "";
        }

        return appendTo(new StringBuilder()).toString();
    }

    public StringBuilder appendTo(final StringBuilder builder)
    {
        if (null == buffer)
        {
            return builder;
        }

        final BookSnapshotDecoder decoder = new BookSnapshotDecoder();
        decoder.wrap(buffer, offset, BLOCK_LENGTH, SCHEMA_VERSION);

        return decoder.appendTo(builder);
    }
}
