fn main() {
    cynic_codegen::register_schema("linear")
        .from_sdl_file("schemas/linear.graphql")
        .expect("failed to load linear.graphql schema file")
        .as_default()
        .expect("failed to register linear schema as default");
}
