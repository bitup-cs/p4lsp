// Test: enum, error, match_kind, header_union, switch, exit, return, abstract

enum EthType {
    IPv4,
    IPv6,
    ARP
}

error {
    InvalidIPv4Header,
    ChecksumError
}

match_kind {
    exact,
    ternary,
    lpm
}

header_union ip_header {
    IPv4_h v4;
    IPv6_h v6;
}

header IPv4_h {
    bit<4> version;
    bit<4> ihl;
    bit<8> diffserv;
    bit<16> totalLen;
}

header IPv6_h {
    bit<4> version;
    bit<8> traffic_class;
    bit<20> flow_label;
}

extern Checksum {
    abstract bit<16> run<T>(in T data);
}

control ingress(inout ip_header hdr) {
    action drop() {
        exit;
    }
    action forward(bit<9> port) {
        return;
    }
    action modify() {
        switch (hdr.v4.version) {
            4: {}
            default: {}
        }
    }
    apply {
        if (hdr.v4.isValid()) {
            return;
        }
        exit;
    }
}
