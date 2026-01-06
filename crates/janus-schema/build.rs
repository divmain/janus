fn main() {
    cynic_codegen::register_schema("linear")
        .from_sdl_file("../../schemas/linear.graphql")
        .expect("Failed to find Linear GraphQL Schema")
        .as_default()
        .expect("Failed to set Linear schema as default");
}
