extern crate syntex_syntax as syntax;

use std::io::prelude::*;
use std::fs::File;
use std::rc::Rc;
use std::ffi::CString;
use std::os::raw::c_char;
use std::collections::*;

use syntax::ast;
use syntax::ast::*;
use syntax::codemap::*;
use syntax::errors::*;
use syntax::parse::{self, token, ParseSess};
use syntax::visit::*;
use syntax::attr::AttrMetaMethods;

enum Status {
    Done,
    Todo
}

#[repr(C)]
struct TodoItem {
    status: Status,
    content: String
}

#[repr(C)]
pub struct ViewModel {
    new_item_content: String,
    items: Vec<TodoItem>
}

#[no_mangle]
pub extern fn test_fn(a: i32, b: i32) -> *const c_char {
    let s = CString::new("Hello from Rust!").unwrap();
    s.into_raw()
}

#[no_mangle]
pub extern fn test_fn3(a: i32) -> *const c_char {
    let s = CString::new("Hello from Rust3!").unwrap();
    s.into_raw()
}

// #[no_mangle]
// pub extern fn add_new_item(vm: &mut ViewModel) {
//     let c = std::mem::replace(&mut vm.new_item_content, "".to_string());
//     let item = TodoItem { status: Status::Todo, content: c};
//     vm.items.push(item);
// }

// #[no_mangle]
// pub extern fn init_vm() -> Box<ViewModel> {
//     let vm = ViewModel {
//         new_item_content: "".to_string(),
//         items: Vec::new()
//     };
//     Box::new(vm)
// }

// #[no_mangle]
// pub extern fn deinit_vm(ptr: *mut ViewModel) {
//     let vm: Box<ViewModel> = unsafe{ std::mem::transmute(ptr) };
// } 

fn main() {
    
    let mut source = String::new();
    let mut f = File::open("./src/main.rs").unwrap();
    let _ = f.read_to_string(&mut source);
    
    let codemap = Rc::new(CodeMap::new());

    let tty_handler = Handler::with_tty_emitter(ColorConfig::Auto,
                                                None,
                                                true,
                                                false,
                                                codemap.clone());

    let parse_session = ParseSess::with_span_handler(tty_handler, codemap.clone());
    
    let krate = parse::parse_crate_from_source_str("adsf".to_string(),
        source.clone(),
        vec![],
        &parse_session).unwrap();

    let visitor = &mut HydraVisitor::new();
    syntax::visit::walk_mod(visitor, &krate.module);
    
    let ret = write_to_csharp(visitor);
    println!("{}", ret);
}

#[derive(Debug)]
struct HydraTypedIdent {
    ident: String,
    ty: String
}

#[derive(Debug)]
struct HydraFunc {
    ident: String,
    args: Vec<HydraTypedIdent>,
    output_ty: String
}

#[derive(Debug)]
struct HydraVisitor {
    func_decls: Vec<HydraFunc>
}

impl<'v> Visitor<'v> for HydraVisitor {
    fn visit_item(&mut self, item: &'v Item) {
        match item.node {
            ItemKind::Fn(ref declaration, unsafety, constness, abi, ref generics, ref body) => {
                let is_c_abi = abi == syntax::abi::Abi::C;
                let is_public = item.vis == syntax::ast::Visibility::Public;
                let is_no_mangle = syntax::attr::contains_name(&item.attrs, "no_mangle");
                
                if is_c_abi && is_public && is_no_mangle {
                    self.process_rust_func(&*item.ident.name.as_str(), declaration);
                }
            }
            ItemKind::Struct(ref struct_definition, ref generics) => {
                let is_rep_c = item.attrs.iter().find(|at| at.check_name("repr")).map_or(false, |i| {
                    if let Some(l) = i.meta_item_list() {
                        syntax::attr::contains_name(l, "C")
                    } else {
                        false
                    }
                });
                
                if is_rep_c {
                    println!("struct def {:?}", &*item.ident.name.as_str());
                    if let VariantData::Struct(ref x, ref y) = *struct_definition {
                        println!("x = {:?}", x);
                    }
                }
            }
            _ => ()
        };
    }
}

impl HydraVisitor {
    fn new() -> Self {
        HydraVisitor {
            func_decls: vec![]
        }
    }
    
    fn process_rust_func(&mut self, ident: &str, fn_decl: &FnDecl) {
        
        let return_type = match fn_decl.output {
            FunctionRetTy::Default(_) => {
                None
            }
            FunctionRetTy::Ty(ref ty) => {
                match ty.node {
                    ast::TyKind::Ptr(ref mut_ty) => {
                        Some("IntPtr")
                    }
                    _ => None
                }
            }
            _ => None
        };
        
        let inputs = fn_decl.inputs.iter().map(|x| {
            let pat_node = &x.pat.node;
            
            let var_name = match *pat_node {
                PatKind::Ident(bm, si, ref op) => {
                    (&*si.node.name.as_str()).to_string()
                    
                }
                _ => "".to_string()
            };
            let ty_node = &x.ty.node;
            let ty_name = match *ty_node {
                TyKind::Path(_, ref p) => {
                    
                    (&*p.segments[0].identifier.name.as_str()).to_string()
                }
                _ => "".to_string()
            };
            
            HydraTypedIdent{
                ident: var_name,
                ty: ty_name
            }
        }).collect::<Vec<_>>();
        
        if let Some(rt) = return_type {
            self.func_decls.push(HydraFunc {
                ident: ident.to_string(),
                args: inputs,
                output_ty: rt.to_string()
            });
        }
    }
}

fn write_to_csharp(hydra: &HydraVisitor) -> String {
    
    
    let mut ret = "// Auto generated by hydra-rs\n\n".to_string();
    
    ret.push_str("class RustWrapper {\n");
    
    for func in &hydra.func_decls {
        
        let f_args = func.args.iter().map(|x| {
            let mut rust_builtin_to_csharp = HashMap::new();
            rust_builtin_to_csharp.insert("i32", "Int32");
            // let () = ;
            let csharp_ty = rust_builtin_to_csharp[x.ty.as_str()];
            
            format!("{} {}", csharp_ty, x.ident)
        }).collect::<Vec<_>>().join(", ");
        
        let common_decl = "private static extern";
        
        
        ret.push_str(format!("[DllImport(\"{}\")]\n", "rlib_test1.dll").as_str());
        let f_decl = format!("{} {} {}({});\n", common_decl, func.output_ty, func.ident, f_args);
        ret.push_str(&f_decl);
    }
    
    ret.push_str("}");
    
    ret
}
