extern crate proc_macro2;

use proc_macro::TokenStream;
#[macro_use]
extern crate quote;

#[proc_macro_derive(ExportPrometheus)]
pub fn derive_field_count(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let ast = syn::parse(input).unwrap();
    parse(&ast)
}

fn parse(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let data = &ast.data;

    let idents: Vec<_> = match data {
        syn::Data::Struct(struct_data) => struct_data
            .fields
            .iter()
            .filter_map(|field| field.ident.as_ref().map(|ident| ident))
            .collect(),
        _ => panic!("Should be derived from struct"),
    };

    let expanded = quote! {
        impl #name {
            pub fn write_prometheus<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
                use core::sync::atomic::Ordering;
                #(solana_prometheus_utils::write_metric(
                    out,
                    &solana_prometheus_utils::MetricFamily {
                        name: &format!("solana_gossip_{}", stringify!(#idents)),
                        help: "Auto generated with Prometheus macro",
                        type_: "counter",
                        metrics: vec![solana_prometheus_utils::Metric::new(self.#idents.0.load(Ordering::Relaxed))],
                    },
                )?;)*
                Ok(())
            }
        }
    };
    expanded.into()
}
