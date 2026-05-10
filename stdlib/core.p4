/*
 * P4 Core Library
 * Built-in types and externs common to all P4 targets.
 */

#ifndef _CORE_P4_
#define _CORE_P4_

// Error codes
error {
    NoError,
    PacketTooShort,
    NoMatch,
    StackOutOfBounds,
    HeaderTooShort,
    ParserTimeout,
    ParserInvalidArgument
}

// Packet-in extern
extern packet_in {
    void extract<T>(out T hdr);
    void extract<T>(out T variableSizeHeader, in bit<32> variableFieldSizeInBits);
    T lookahead<T>();
    void advance(in bit<32> sizeInBits);
    bit<32> length();
}

// Packet-out extern
extern packet_out {
    void emit<T>(in T hdr);
}

// Built-in match kinds
match_kind {
    exact,
    ternary,
    lpm
}

#endif
