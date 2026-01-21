use std::fs;
use std::time::{Duration, Instant};

use testcontainers::GenericImage;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::SyncRunner;

/// Ensures the OTLP exporter issues trace requests to the configured endpoint.
#[test]
fn otel_exporter_emits_traces_to_endpoint() {
    let image = GenericImage::new("mendhak/http-https-echo", "31")
        .with_exposed_port(80.tcp())
        .with_wait_for(WaitFor::seconds(1));
    let container = image.start().expect("start echo container");
    let host_port = container.get_host_port_ipv4(80).expect("resolve host port");
    let endpoint = format!("http://127.0.0.1:{host_port}");

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let class_path = temp_dir.path().join("Sample.class");
    fs::write(&class_path, build_class_bytes()).expect("write class file");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_inspequte"))
        .arg("--input")
        .arg(&class_path)
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint)
        .output()
        .expect("run inspequte");
    assert!(
        output.status.success(),
        "inspequte failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let start = Instant::now();
    loop {
        let stdout_bytes = container.stdout_to_vec().expect("read container stdout");
        let stderr_bytes = container.stderr_to_vec().expect("read container stderr");
        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        if stdout.contains("/v1/traces") || stderr.contains("/v1/traces") {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            panic!(
                "expected OTLP traces request in container logs; stdout={stdout} stderr={stderr}"
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Minimal class file writer for OTLP exporter testing.
struct ClassFileBuilder {
    cp: Vec<CpEntry>,
    this_class: u16,
    super_class: u16,
    methods: Vec<MethodSpec>,
    code_index: u16,
}

impl ClassFileBuilder {
    fn new(class_name: &str, super_name: &str) -> Self {
        let mut builder = Self {
            cp: Vec::new(),
            this_class: 0,
            super_class: 0,
            methods: Vec::new(),
            code_index: 0,
        };
        builder.code_index = builder.add_utf8("Code");
        builder.this_class = builder.add_class(class_name);
        builder.super_class = builder.add_class(super_name);
        builder
    }

    fn add_utf8(&mut self, value: &str) -> u16 {
        self.cp.push(CpEntry::Utf8(value.to_string()));
        self.cp.len() as u16
    }

    fn add_class(&mut self, name: &str) -> u16 {
        let name_index = self.add_utf8(name);
        self.cp.push(CpEntry::Class(name_index));
        self.cp.len() as u16
    }

    fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let name_index = self.add_utf8(name);
        let descriptor_index = self.add_utf8(descriptor);
        self.cp
            .push(CpEntry::NameAndType(name_index, descriptor_index));
        self.cp.len() as u16
    }

    fn add_method_ref(&mut self, class: &str, name: &str, descriptor: &str) -> u16 {
        let class_index = self.add_class(class);
        let name_and_type = self.add_name_and_type(name, descriptor);
        self.cp.push(CpEntry::MethodRef(class_index, name_and_type));
        self.cp.len() as u16
    }

    fn add_method(
        &mut self,
        name: &str,
        descriptor: &str,
        code: Vec<u8>,
        max_stack: u16,
        max_locals: u16,
    ) {
        let name_index = self.add_utf8(name);
        let descriptor_index = self.add_utf8(descriptor);
        self.methods.push(MethodSpec {
            name_index,
            descriptor_index,
            code,
            max_stack,
            max_locals,
        });
    }

    fn finish(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_u32(&mut bytes, 0xCAFEBABE);
        write_u16(&mut bytes, 0);
        write_u16(&mut bytes, 52);
        write_u16(&mut bytes, (self.cp.len() + 1) as u16);
        for entry in &self.cp {
            entry.write(&mut bytes);
        }
        write_u16(&mut bytes, 0x0021);
        write_u16(&mut bytes, self.this_class);
        write_u16(&mut bytes, self.super_class);
        write_u16(&mut bytes, 0);
        write_u16(&mut bytes, 0);
        write_u16(&mut bytes, self.methods.len() as u16);
        for method in &self.methods {
            write_u16(&mut bytes, 0x0001);
            write_u16(&mut bytes, method.name_index);
            write_u16(&mut bytes, method.descriptor_index);
            write_u16(&mut bytes, 1);
            write_u16(&mut bytes, self.code_index);
            let attr_len = 12 + method.code.len() as u32;
            write_u32(&mut bytes, attr_len);
            write_u16(&mut bytes, method.max_stack);
            write_u16(&mut bytes, method.max_locals);
            write_u32(&mut bytes, method.code.len() as u32);
            bytes.extend_from_slice(&method.code);
            write_u16(&mut bytes, 0);
            write_u16(&mut bytes, 0);
        }
        write_u16(&mut bytes, 0);
        bytes
    }
}

/// Method definition for generated class files.
struct MethodSpec {
    name_index: u16,
    descriptor_index: u16,
    code: Vec<u8>,
    max_stack: u16,
    max_locals: u16,
}

/// Constant pool entries needed by generated class files.
enum CpEntry {
    Utf8(String),
    Class(u16),
    NameAndType(u16, u16),
    MethodRef(u16, u16),
}

impl CpEntry {
    fn write(&self, bytes: &mut Vec<u8>) {
        match self {
            CpEntry::Utf8(value) => {
                bytes.push(1);
                write_u16(bytes, value.len() as u16);
                bytes.extend_from_slice(value.as_bytes());
            }
            CpEntry::Class(name_index) => {
                bytes.push(7);
                write_u16(bytes, *name_index);
            }
            CpEntry::NameAndType(name_index, descriptor_index) => {
                bytes.push(12);
                write_u16(bytes, *name_index);
                write_u16(bytes, *descriptor_index);
            }
            CpEntry::MethodRef(class_index, name_and_type) => {
                bytes.push(10);
                write_u16(bytes, *class_index);
                write_u16(bytes, *name_and_type);
            }
        }
    }
}

fn build_class_bytes() -> Vec<u8> {
    let mut builder = ClassFileBuilder::new("Sample", "java/lang/Object");
    let object_init = builder.add_method_ref("java/lang/Object", "<init>", "()V");
    let init_code = vec![0x2a, 0xb7, high(object_init), low(object_init), 0xb1];
    builder.add_method("<init>", "()V", init_code, 1, 1);
    builder.finish()
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn high(value: u16) -> u8 {
    (value >> 8) as u8
}

fn low(value: u16) -> u8 {
    (value & 0xff) as u8
}
