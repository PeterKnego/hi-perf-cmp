/* Generated SBE (Simple Binary Encoding) message codec. */
package booksnap;

import org.agrona.DirectBuffer;

@SuppressWarnings("all")
public final class BookSnapshotDecoder
{
    public static final int BLOCK_LENGTH = 36;
    public static final int TEMPLATE_ID = 1;
    public static final int SCHEMA_ID = 8;
    public static final int SCHEMA_VERSION = 1;
    public static final String SEMANTIC_VERSION = "";
    public static final java.nio.ByteOrder BYTE_ORDER = java.nio.ByteOrder.LITTLE_ENDIAN;

    private final BookSnapshotDecoder parentMessage = this;
    private DirectBuffer buffer;
    private int offset;
    private int limit;
    int actingBlockLength;
    int actingVersion;

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

    public DirectBuffer buffer()
    {
        return buffer;
    }

    public int offset()
    {
        return offset;
    }

    public BookSnapshotDecoder wrap(
        final DirectBuffer buffer,
        final int offset,
        final int actingBlockLength,
        final int actingVersion)
    {
        if (buffer != this.buffer)
        {
            this.buffer = buffer;
        }
        this.offset = offset;
        this.actingBlockLength = actingBlockLength;
        this.actingVersion = actingVersion;
        limit(offset + actingBlockLength);

        return this;
    }

    public BookSnapshotDecoder wrapAndApplyHeader(
        final DirectBuffer buffer,
        final int offset,
        final MessageHeaderDecoder headerDecoder)
    {
        headerDecoder.wrap(buffer, offset);

        final int templateId = headerDecoder.templateId();
        if (TEMPLATE_ID != templateId)
        {
            throw new IllegalStateException("Invalid TEMPLATE_ID: " + templateId);
        }

        return wrap(
            buffer,
            offset + MessageHeaderDecoder.ENCODED_LENGTH,
            headerDecoder.blockLength(),
            headerDecoder.version());
    }

    public BookSnapshotDecoder sbeRewind()
    {
        return wrap(buffer, offset, actingBlockLength, actingVersion);
    }

    public int sbeDecodedLength()
    {
        final int currentLimit = limit();
        sbeSkip();
        final int decodedLength = encodedLength();
        limit(currentLimit);

        return decodedLength;
    }

    public int actingVersion()
    {
        return actingVersion;
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

    public long priceMin()
    {
        return buffer.getLong(offset + 0, BYTE_ORDER);
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

    public long tickSize()
    {
        return buffer.getLong(offset + 8, BYTE_ORDER);
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

    public long nLevels()
    {
        return (buffer.getInt(offset + 16, BYTE_ORDER) & 0xFFFF_FFFFL);
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

    public long capacity()
    {
        return (buffer.getInt(offset + 20, BYTE_ORDER) & 0xFFFF_FFFFL);
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

    public long hwm()
    {
        return (buffer.getInt(offset + 24, BYTE_ORDER) & 0xFFFF_FFFFL);
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

    public int bestBid()
    {
        return buffer.getInt(offset + 28, BYTE_ORDER);
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

    public int bestAsk()
    {
        return buffer.getInt(offset + 32, BYTE_ORDER);
    }


    private final LevelsDecoder levels = new LevelsDecoder(this);

    public static long levelsDecoderId()
    {
        return 10;
    }

    public static int levelsDecoderSinceVersion()
    {
        return 0;
    }

    public LevelsDecoder levels()
    {
        levels.wrap(buffer);
        return levels;
    }

    public static final class LevelsDecoder
        implements Iterable<LevelsDecoder>, java.util.Iterator<LevelsDecoder>
    {
        public static final int HEADER_SIZE = 4;
        private final BookSnapshotDecoder parentMessage;
        private DirectBuffer buffer;
        private int count;
        private int index;
        private int offset;
        private int blockLength;

        LevelsDecoder(final BookSnapshotDecoder parentMessage)
        {
            this.parentMessage = parentMessage;
        }

        public void wrap(final DirectBuffer buffer)
        {
            if (buffer != this.buffer)
            {
                this.buffer = buffer;
            }

            index = 0;
            final int limit = parentMessage.limit();
            parentMessage.limit(limit + HEADER_SIZE);
            blockLength = (buffer.getShort(limit + 0, BYTE_ORDER) & 0xFFFF);
            count = (buffer.getShort(limit + 2, BYTE_ORDER) & 0xFFFF);
        }

        public LevelsDecoder next()
        {
            if (index >= count)
            {
                throw new java.util.NoSuchElementException();
            }

            offset = parentMessage.limit();
            parentMessage.limit(offset + blockLength);
            ++index;

            return this;
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

        public int actingBlockLength()
        {
            return blockLength;
        }

        public int actingVersion()
        {
            return parentMessage.actingVersion;
        }

        public int count()
        {
            return count;
        }

        public java.util.Iterator<LevelsDecoder> iterator()
        {
            return this;
        }

        public void remove()
        {
            throw new UnsupportedOperationException();
        }

        public boolean hasNext()
        {
            return index < count;
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

        public short sideRaw()
        {
            return ((short)(buffer.getByte(offset + 0) & 0xFF));
        }

        public Side side()
        {
            return Side.get(((short)(buffer.getByte(offset + 0) & 0xFF)));
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

        public long levelTick()
        {
            return (buffer.getInt(offset + 1, BYTE_ORDER) & 0xFFFF_FFFFL);
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

        public long qtyTotal()
        {
            return buffer.getLong(offset + 5, BYTE_ORDER);
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

        public long orderCount()
        {
            return (buffer.getInt(offset + 13, BYTE_ORDER) & 0xFFFF_FFFFL);
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

        public long head()
        {
            return (buffer.getInt(offset + 17, BYTE_ORDER) & 0xFFFF_FFFFL);
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

        public long tail()
        {
            return (buffer.getInt(offset + 21, BYTE_ORDER) & 0xFFFF_FFFFL);
        }


        public StringBuilder appendTo(final StringBuilder builder)
        {
            if (null == buffer)
            {
                return builder;
            }

            builder.append('(');
            builder.append("side=");
            builder.append(this.side());
            builder.append('|');
            builder.append("levelTick=");
            builder.append(this.levelTick());
            builder.append('|');
            builder.append("qtyTotal=");
            builder.append(this.qtyTotal());
            builder.append('|');
            builder.append("orderCount=");
            builder.append(this.orderCount());
            builder.append('|');
            builder.append("head=");
            builder.append(this.head());
            builder.append('|');
            builder.append("tail=");
            builder.append(this.tail());
            builder.append(')');

            return builder;
        }
        
        public LevelsDecoder sbeSkip()
        {

            return this;
        }
    }

    private final OrdersDecoder orders = new OrdersDecoder(this);

    public static long ordersDecoderId()
    {
        return 20;
    }

    public static int ordersDecoderSinceVersion()
    {
        return 0;
    }

    public OrdersDecoder orders()
    {
        orders.wrap(buffer);
        return orders;
    }

    public static final class OrdersDecoder
        implements Iterable<OrdersDecoder>, java.util.Iterator<OrdersDecoder>
    {
        public static final int HEADER_SIZE = 4;
        private final BookSnapshotDecoder parentMessage;
        private DirectBuffer buffer;
        private int count;
        private int index;
        private int offset;
        private int blockLength;

        OrdersDecoder(final BookSnapshotDecoder parentMessage)
        {
            this.parentMessage = parentMessage;
        }

        public void wrap(final DirectBuffer buffer)
        {
            if (buffer != this.buffer)
            {
                this.buffer = buffer;
            }

            index = 0;
            final int limit = parentMessage.limit();
            parentMessage.limit(limit + HEADER_SIZE);
            blockLength = (buffer.getShort(limit + 0, BYTE_ORDER) & 0xFFFF);
            count = (buffer.getShort(limit + 2, BYTE_ORDER) & 0xFFFF);
        }

        public OrdersDecoder next()
        {
            if (index >= count)
            {
                throw new java.util.NoSuchElementException();
            }

            offset = parentMessage.limit();
            parentMessage.limit(offset + blockLength);
            ++index;

            return this;
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

        public int actingBlockLength()
        {
            return blockLength;
        }

        public int actingVersion()
        {
            return parentMessage.actingVersion;
        }

        public int count()
        {
            return count;
        }

        public java.util.Iterator<OrdersDecoder> iterator()
        {
            return this;
        }

        public void remove()
        {
            throw new UnsupportedOperationException();
        }

        public boolean hasNext()
        {
            return index < count;
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

        public long slot()
        {
            return (buffer.getInt(offset + 0, BYTE_ORDER) & 0xFFFF_FFFFL);
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

        public long orderId()
        {
            return buffer.getLong(offset + 4, BYTE_ORDER);
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

        public long price()
        {
            return buffer.getLong(offset + 12, BYTE_ORDER);
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

        public long qty()
        {
            return buffer.getLong(offset + 20, BYTE_ORDER);
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

        public long filled()
        {
            return buffer.getLong(offset + 28, BYTE_ORDER);
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

        public short sideRaw()
        {
            return ((short)(buffer.getByte(offset + 36) & 0xFF));
        }

        public Side side()
        {
            return Side.get(((short)(buffer.getByte(offset + 36) & 0xFF)));
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

        public long nextSlot()
        {
            return (buffer.getInt(offset + 37, BYTE_ORDER) & 0xFFFF_FFFFL);
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

        public long prev()
        {
            return (buffer.getInt(offset + 41, BYTE_ORDER) & 0xFFFF_FFFFL);
        }


        public StringBuilder appendTo(final StringBuilder builder)
        {
            if (null == buffer)
            {
                return builder;
            }

            builder.append('(');
            builder.append("slot=");
            builder.append(this.slot());
            builder.append('|');
            builder.append("orderId=");
            builder.append(this.orderId());
            builder.append('|');
            builder.append("price=");
            builder.append(this.price());
            builder.append('|');
            builder.append("qty=");
            builder.append(this.qty());
            builder.append('|');
            builder.append("filled=");
            builder.append(this.filled());
            builder.append('|');
            builder.append("side=");
            builder.append(this.side());
            builder.append('|');
            builder.append("nextSlot=");
            builder.append(this.nextSlot());
            builder.append('|');
            builder.append("prev=");
            builder.append(this.prev());
            builder.append(')');

            return builder;
        }
        
        public OrdersDecoder sbeSkip()
        {

            return this;
        }
    }

    public String toString()
    {
        if (null == buffer)
        {
            return "";
        }

        final BookSnapshotDecoder decoder = new BookSnapshotDecoder();
        decoder.wrap(buffer, offset, actingBlockLength, actingVersion);

        return decoder.appendTo(new StringBuilder()).toString();
    }

    public StringBuilder appendTo(final StringBuilder builder)
    {
        if (null == buffer)
        {
            return builder;
        }

        final int originalLimit = limit();
        limit(offset + actingBlockLength);
        builder.append("[BookSnapshot](sbeTemplateId=");
        builder.append(TEMPLATE_ID);
        builder.append("|sbeSchemaId=");
        builder.append(SCHEMA_ID);
        builder.append("|sbeSchemaVersion=");
        if (parentMessage.actingVersion != SCHEMA_VERSION)
        {
            builder.append(parentMessage.actingVersion);
            builder.append('/');
        }
        builder.append(SCHEMA_VERSION);
        builder.append("|sbeBlockLength=");
        if (actingBlockLength != BLOCK_LENGTH)
        {
            builder.append(actingBlockLength);
            builder.append('/');
        }
        builder.append(BLOCK_LENGTH);
        builder.append("):");
        builder.append("priceMin=");
        builder.append(this.priceMin());
        builder.append('|');
        builder.append("tickSize=");
        builder.append(this.tickSize());
        builder.append('|');
        builder.append("nLevels=");
        builder.append(this.nLevels());
        builder.append('|');
        builder.append("capacity=");
        builder.append(this.capacity());
        builder.append('|');
        builder.append("hwm=");
        builder.append(this.hwm());
        builder.append('|');
        builder.append("bestBid=");
        builder.append(this.bestBid());
        builder.append('|');
        builder.append("bestAsk=");
        builder.append(this.bestAsk());
        builder.append('|');
        builder.append("levels=[");
        final int levelsOriginalOffset = levels.offset;
        final int levelsOriginalIndex = levels.index;
        final LevelsDecoder levels = this.levels();
        if (levels.count() > 0)
        {
            while (levels.hasNext())
            {
                levels.next().appendTo(builder);
                builder.append(',');
            }
            builder.setLength(builder.length() - 1);
        }
        levels.offset = levelsOriginalOffset;
        levels.index = levelsOriginalIndex;
        builder.append(']');
        builder.append('|');
        builder.append("orders=[");
        final int ordersOriginalOffset = orders.offset;
        final int ordersOriginalIndex = orders.index;
        final OrdersDecoder orders = this.orders();
        if (orders.count() > 0)
        {
            while (orders.hasNext())
            {
                orders.next().appendTo(builder);
                builder.append(',');
            }
            builder.setLength(builder.length() - 1);
        }
        orders.offset = ordersOriginalOffset;
        orders.index = ordersOriginalIndex;
        builder.append(']');

        limit(originalLimit);

        return builder;
    }
    
    public BookSnapshotDecoder sbeSkip()
    {
        sbeRewind();
        LevelsDecoder levels = this.levels();
        if (levels.count() > 0)
        {
            while (levels.hasNext())
            {
                levels.next();
                levels.sbeSkip();
            }
        }
        OrdersDecoder orders = this.orders();
        if (orders.count() > 0)
        {
            while (orders.hasNext())
            {
                orders.next();
                orders.sbeSkip();
            }
        }

        return this;
    }
}
