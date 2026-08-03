fn main() {
    topcoat::tailwind::BuildConfig::new()
        .input("styles.css")
        .output("assets/styles.generated.css")
        .render()
        .expect("failed to build stylesheet");
}
