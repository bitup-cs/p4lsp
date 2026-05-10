// Test: cast expression

control test() {
    action a() {
        bit<32> x = 1;
        bit<16> y = (bit<16>)x;
        bit<8> z = (bit<8>)(x + 1);
    }
    apply {}
}
