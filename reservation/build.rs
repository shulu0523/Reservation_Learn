fn main() {
    tonic_build::compile_protos("proto/reservation.proto")
        .expect("Failed to compile proto files");
}