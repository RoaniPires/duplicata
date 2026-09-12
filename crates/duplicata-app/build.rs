fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../../assets/duplicata.ico");
        println!("cargo:rerun-if-changed=duplicata.manifest");
        winres::WindowsResource::new()
            .set_icon("../../assets/duplicata.ico")
            .set_manifest_file("duplicata.manifest")
            .compile()
            .expect("falha ao compilar os recursos Win32 (ícone + manifesto DPI)");
    }
}
