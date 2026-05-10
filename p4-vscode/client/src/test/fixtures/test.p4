struct ethernet_t {
    bit<48> dstAddr;
    bit<48> srcAddr;
    bit<16> etherType;
}

struct ipv4_t {
    bit<32> srcAddr;
    bit<32> dstAddr;
}

error {
    InvalidHeader,
    ChecksumError
}

enum Priority {
    Low,
    Medium,
    High
}

extern Checksum16 {
    Checksum16();
    bit<16> get();
    void clear();
}

control MyCtl(inout ethernet_t eth) {
    apply {
        bit<32> local_var = 0;
        bool flag = true;
        // Type mismatch test: bool = bit<32>
        bool mismatch = local_var;
        eth.dstAddr = 1;
    }
}

parser MyParser(packet_in pkt) {
    state start {
        ethernet_t hdr;
        transition accept;
    }
}

action drop() {
    // Drop action
}

table t {
    key = {
        eth.etherType: exact;
    }
    actions = {
        drop;
    }
    default_action = drop;
}
