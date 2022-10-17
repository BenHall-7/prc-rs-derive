extern crate proc_macro;
use crate::proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro2::TokenStream as Tokens;
use quote::spanned::Spanned;
use quote::ToTokens;
use quote::{format_ident, quote};
use std::iter::Peekable;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Eq;
use syn::token::{Comma, Where};
use syn::FieldsNamed;
use syn::Lit;
use syn::Path;
use syn::{
    parenthesized, parse, Attribute, Data, DataEnum, DeriveInput, Error, Field, Fields,
    GenericParam, Generics, Ident, ImplGenerics, Index, Result as SynResult, TypeGenerics,
    TypeParam, Visibility, WhereClause, WherePredicate,
};

const NAMED_STRUCT_ONLY_ERR: &str = "Derive macro only implemented for named structs";

const INVALID_ATTR_NAME: &str = "Invalid struct attribute. Accepted name is only 'path'";
const INVALID_ATTR_COUNT: &str =
    "Invalid struct attributes. Only use 'path' attribute in struct once";

const INVALID_FIELD_ATTR_NAME: &str =
    "Invalid field attribute. Accepted attribute names are 'name', and 'hash'";
const INVALID_FIELD_ATTR_COUNT: &str =
    "Invalid field attributes. Only use 'name' or 'hash' attribute in field once";

#[proc_macro_derive(Prc, attributes(prc))]
pub fn prc_derive(input: TokenStream) -> TokenStream {
    match derive_or_error(input) {
        Err(err) => err.to_compile_error().into(),
        Ok(result) => result,
    }
}

fn derive_or_error(input: TokenStream) -> SynResult<TokenStream> {
    let input: DeriveInput = syn::parse(input)?;
    let ident = input.ident;

    let attrs = parse_struct_attributes(&input.attrs)?;

    match input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => derive_named_struct(ident, &attrs, &fields.named),
            Fields::Unnamed(fields) => panic!("{}", NAMED_STRUCT_ONLY_ERR),
            Fields::Unit => panic!("{}", NAMED_STRUCT_ONLY_ERR),
        },
        _ => panic!("{}", NAMED_STRUCT_ONLY_ERR),
    }
}

#[derive(Default)]
struct MainAttributes {
    path: Option<Path>,
}

enum MainAttribute {
    Path(Path),
}

enum FieldAttribute {
    // in case we want something more general later, add this
    // From(Tokens),
    Name(Lit),
    Hash(Lit),
}

fn parse_struct_attributes(attrs: &[Attribute]) -> SynResult<MainAttributes> {
    let mut attributes = MainAttributes::default();

    attrs
        .iter()
        .filter(|attr| attr.path.is_ident("prc"))
        .try_for_each(|attr| {
            let attr_kind: MainAttribute = attr.parse_args()?;
            match attr_kind {
                MainAttribute::Path(path) => {
                    if attributes.path.is_some() {
                        panic!("{}", INVALID_ATTR_COUNT);
                    } else {
                        attributes.path = Some(path);
                    }
                }
            }

            SynResult::Ok(())
        })?;

    Ok(attributes)
}

impl Parse for MainAttribute {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let key: Ident = input.parse()?;
        let struct_attr = match key.to_string().as_ref() {
            "path" => {
                let _eq: Eq = input.parse()?;
                MainAttribute::Path(input.parse()?)
            }
            _ => panic!("{}", INVALID_ATTR_NAME),
        };

        SynResult::Ok(struct_attr)
    }
}

impl Parse for FieldAttribute {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let key: Ident = input.parse()?;
        match key.to_string().as_ref() {
            "name" => {
                let _eq: Eq = input.parse()?;
                Ok(FieldAttribute::Name(input.parse()?))
            }
            "hash" => {
                let _eq: Eq = input.parse()?;
                Ok(FieldAttribute::Hash(input.parse()?))
            }
            // "from" => {}
            _ => Err(input.error(INVALID_FIELD_ATTR_NAME)),
        }
    }
}

fn derive_named_struct(
    ident: Ident,
    attrs: &MainAttributes,
    fields: &Punctuated<Field, Comma>,
) -> SynResult<TokenStream> {
    let path = attrs
        .path
        .as_ref()
        .map(|some_path| quote!(#some_path))
        .unwrap_or(quote!(::prc));

    let names = fields
        .iter()
        .map(|field| {
            let attrs = field
                .attrs
                .iter()
                .filter(|attr| attr.path.is_ident("prc"))
                .map(|attr| attr.parse_args())
                .collect::<Result<Vec<_>, _>>()?;

            if attrs.len() > 1 {
                panic!("{}", INVALID_FIELD_ATTR_COUNT);
            }

            let ident = field.ident.as_ref().unwrap();
            let ident_string = ident.to_string();

            let hash = match attrs.get(0) {
                Some(FieldAttribute::Hash(hash)) => quote!(#path::hash40::Hash40(#hash)),
                Some(FieldAttribute::Name(name)) => quote!(#path::hash40::hash40(#name)),
                None => quote!(#path::hash40::hash40(#ident_string)),
            };

            Ok((ident, hash))
        })
        .collect::<SynResult<Vec<_>>>()?;

    let struct_names = names.iter().map(|name| name.0);
    let hashes = names.iter().map(|name| &name.1);

    Ok(quote! {
        impl Prc for #ident {
            fn read_param<R: ::std::io::Read + ::std::io::Seek>(reader: &mut R, offsets: #path::FileOffsets) -> #path::Result<Self> {
                let data = #path::StructData::from_stream(reader)?;
                Ok(Self {
                    #(
                        #struct_names: data.read_child(reader, #hashes, offsets)?,
                    )*
                })
            }
        }
    }
    .into())
}
