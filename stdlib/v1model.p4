/*
 * v1model.p4
 * P4-16 standard architecture for software targets.
 */

#ifndef _V1MODEL_P4_
#define _V1MODEL_P4_

#include <core.p4>

extern counter {
    counter(bit<32> size, CounterType type);
    void count(in bit<32> index);
}

extern direct_counter {
    direct_counter(CounterType type);
    void count();
}

extern meter {
    meter(bit<32> size, MeterType type);
    void execute_meter<T>(in bit<32> index, out T result);
}

extern direct_meter<T> {
    direct_meter(MeterType type);
    void read(out T result);
}

extern register<T> {
    register(bit<32> size);
    void read(out T result, in bit<32> index);
    void write(in bit<32> index, in T value);
}

extern action_profile {
    action_profile(bit<32> size);
}

extern action_selector {
    action_selector(HashAlgorithm algorithm, bit<32> size, bit<32> outputWidth);
}

extern Hash {
    Hash(HashAlgorithm algo);
    bit<16> get_hash<T>(in T data);
}

extern Checksum16 {
    Checksum16();
    bit<16> get<T>(in T data);
}

enum CounterType {
    packets,
    bytes,
    packets_and_bytes
}

enum MeterType {
    packets,
    bytes
}

enum HashAlgorithm {
    crc32,
    crc32_custom,
    identity,
    csum16,
    xor16,
    random,
    crc16,
    crc64,
    xxhash64,
    crc32c,
    murmur3,
    jenkins,
    csum_custom,
    crc32c_custom
}

struct standard_metadata_t {
    bit<9>  ingress_port;
    bit<9>  egress_spec;
    bit<9>  egress_port;
    bit<32> instance_type;
    bit<32> packet_length;
    bit<32> enq_timestamp;
    bit<19> enq_qdepth;
    bit<32> deq_timedelta;
    bit<19> deq_qdepth;
    bit<48> ingress_global_timestamp;
    bit<48> egress_global_timestamp;
    bit<16> mcast_grp;
    bit<16> checksum;
    bit<3>  priority;
    bit<1>  clone_spec;
    bit<1>  deflection;
    bit<32> ingress_timestamp;
    bit<32> egress_timestamp;
    bit<1>  drop;
    bit<16> recirculate_port;
    bit<1>  recirculate_flag;
    bit<32> parser_error;
    bit<1>  checksum_error;
    bit<32> mark_to_drop;
}

parser Parser<H, M>(
    packet_in b,
    out H parsedHeaders,
    inout M meta,
    inout standard_metadata_t standard_metadata);

control VerifyChecksum<H, M>(
    inout H hdr,
    inout M meta);

control Ingress<H, M>(
    inout H hdr,
    inout M meta,
    inout standard_metadata_t standard_metadata);

control Egress<H, M>(
    inout H hdr,
    inout M meta,
    inout standard_metadata_t standard_metadata);

control ComputeChecksum<H, M>(
    inout H hdr,
    inout M meta);

control Deparser<H>(
    packet_out b,
    in H hdr);

package V1Switch<H, M>(
    Parser<H, M> p,
    VerifyChecksum<H, M> vr,
    Ingress<H, M> ig,
    Egress<H, M> eg,
    ComputeChecksum<H, M> ck,
    Deparser<H> dp);

#endif
