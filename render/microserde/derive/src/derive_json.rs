use proc_macro::{TokenStream};
use crate::macro_lib::*;

pub fn derive_ser_json_impl(input: TokenStream) -> TokenStream {

    let mut parser = TokenParser::new(input);
    let mut tb = TokenBuilder::new();
    
    parser.eat_ident("pub");
    if parser.eat_ident("struct"){
        if let Some(name) = parser.eat_any_ident(){
            
            let generic = parser.eat_generic();
            let types = parser.eat_all_types();
            let where_clause = parser.eat_where_clause(Some("SerJson"));

            tb.add("impl").stream(generic.clone());
            tb.add("SerJson for").ident(&name).stream(generic).stream(where_clause);
            tb.add("{ fn ser_json ( & self , d : usize , s : & mut makepad_microserde :: SerJsonState ) {");
            
            if let Some(types) = types{
                tb.add("s . out . push (").chr('[').add(") ;");
                for i in 0..types.len(){
                     tb.add("self .").unsuf_usize(i).add(". ser_json ( d , s ) ;");
                     if i != types.len() - 1{
                         tb.add("s . out . push (").chr(',').add(") ;");
                     }
                }
                tb.add("s . out . push (").chr(']').add(") ;");
            }
            else if let Some(fields) = parser.eat_all_struct_fields(){ 
                tb.add("s . st_pre ( ) ;");
                // named struct
                for (field,ty) in fields{
                    if ty.into_iter().next().unwrap().to_string() == "Option"{
                        tb.add("if let Some ( t ) = ").add("& self .").ident(&field).add("{");
                        tb.add("s . field ( d + 1 ,").string(&field).add(") ;");
                        tb.add("t . ser_json ( d + 1 , s ) ; s . conl ( ) ; } ;");
                    }
                    else{
                        tb.add("s . field ( d + 1 ,").string(&field).add(" ) ;");
                        tb.add("self .").ident(&field).add(". ser_json ( d + 1 , s ) ; s . conl ( ) ;");
                    }
                }
                tb.add("s . st_post ( d ) ;");
            }
            else{
                return parser.unexpected()
            }
            tb.add("} } ;");
            return tb.end();
        }
    }
    else if parser.eat_ident("enum"){
        if let Some(name) = parser.eat_any_ident(){
            let generic = parser.eat_generic();
            let where_clause = parser.eat_where_clause(Some("SerJson"));

            tb.add("impl").stream(generic.clone());
            tb.add("SerJson for").ident(&name).stream(generic).stream(where_clause);
            tb.add("{ fn ser_json ( & self , d : usize , s : & mut makepad_microserde :: SerJsonState ) {");
            tb.add("match self {");
            
            if !parser.open_brace(){
                return parser.unexpected()
            }

            while !parser.eat_eot(){
                // parse ident
                if let Some(variant) = parser.eat_any_ident(){
                    if let Some(types) = parser.eat_all_types(){
                        
                        tb.add("Self ::").ident(&variant).add("(");
                        for i in 0..types.len(){
                            tb.ident(&format!("n{}", i)).add(",");
                        }
                        tb.add(") => {");
                        tb.add("s . label (").string(&variant).add(") ;");
                        tb.add("s . out . push (").chr(':').add(") ;");
                        tb.add("s . out . push (").chr('[').add(") ;");
                        
                        for i in 0..types.len(){
                            tb.ident(&format!("n{}", i)).add(". ser_json ( d , s ) ;");
                            if i != types.len() - 1{
                                tb.add("s . out . push (").chr(',').add(") ;");
                            }
                        }
                        tb.add("s . out . push (").chr(']').add(") ;");
                        tb.add("}");
                    }
                    else if let Some(fields) = parser.eat_all_struct_fields(){ // named variant
                        tb.add("Self ::").ident(&variant).add("{");
                        for (field, _ty) in fields.iter(){
                            tb.ident(field).add(",");
                        }
                        tb.add("} => {");
                        
                        tb.add("s . label (").string(&variant).add(") ;");
                        tb.add("s . out . push (").chr(':').add(") ;");
                        tb.add("s . st_pre ( ) ;");
                        
                        for (field, ty) in fields{
                            if ty.into_iter().next().unwrap().to_string() == "Option"{
                                tb.add("if let Some ( t ) = ").ident(&field).add("{");
                                tb.add("s . field ( d + 1 ,").string(&field).add(") ;");
                                tb.add("t . ser_json ( d + 1 , s ) ; } ;");
                            }
                            else{
                                tb.add("s . field ( d + 1 ,").string(&field).add(" ) ;");
                                tb.ident(&field).add(". ser_json ( d + 1 , s ) ;");
                            }
                        }
                        tb.add("s . st_post ( d ) ; }");
                    }
                    else if parser.is_punct(',') || parser.is_eot(){ // bare variant
                        tb.add("Self ::").ident(&variant).add("=> {");
                        tb.add("s . label (").string(&variant).add(") ;");
                        tb.add("s . out . push_str (").string(":[]").add(") ; }");
                    }
                    else{
                        return parser.unexpected();
                    }
                    parser.eat_punct(',');
                }
                else{
                    return parser.unexpected()
                }
            }
            tb.add("} } } ;");
            return tb.end();
        }
    }
    return parser.unexpected()
}
/*
#[proc_macro_derive(DeBin)]
pub fn derive_de_bin(input: TokenStream) -> TokenStream {
    let mut parser = TokenParser::new(input);
    let mut tb = TokenBuilder::new();
    
    parser.eat_ident("pub");
    if parser.eat_ident("struct"){
        if let Some(name) = parser.eat_any_ident(){
            let generic = parser.eat_generic();
            let types = parser.eat_all_types();
            let where_clause = parser.eat_where_clause(Some("SerBin"));

            tb.add("impl").stream(generic.clone());
            tb.add("DeBin for").ident(&name).stream(generic).stream(where_clause);
            tb.add("{ fn de_bin ( o : & mut usize , d : & [ u8 ] )");
            tb.add("-> std :: result :: Result < Self , DeBinErr > { ");
            tb.add("std :: result :: Result :: Ok ( Self");

            if let Some(types) = types{
                tb.add("(");
                for _ in 0..types.len(){
                     tb.add("DeBin :: de_bin ( o , d ) ?");
                }
                tb.add(")");
            }
            else if let Some(fields) = parser.eat_all_struct_fields(){ 
                tb.add("{");
                for (field,_ty) in fields{
                    tb.ident(&field).add(": DeBin :: de_bin ( o , d ) ? ,");
                }
                tb.add("}");
            }
            else{
                return parser.unexpected()
            }
            tb.add(") } } ;"); 
            return tb.end();
        }
    }
    else if parser.eat_ident("enum"){
        if let Some(name) = parser.eat_any_ident(){
            let generic = parser.eat_generic();
            let where_clause = parser.eat_where_clause(Some("DeBin"));
            
            tb.add("impl").stream(generic.clone());
            tb.add("DeBin for").ident(&name).stream(generic).stream(where_clause);
            tb.add("{ fn de_bin ( o : & mut usize , d : & [ u8 ] )");
            tb.add("-> std :: result :: Result < Self , DeBinErr > {");
            tb.add("let id : u16 = DeBin :: de_bin ( o , d ) ? ;");
            tb.add("match id {");
            
            if !parser.open_brace(){
                return parser.unexpected()
            }
            let mut index = 0;
            while !parser.eat_eot(){
                // parse ident
                if let Some(variant) = parser.eat_any_ident(){
                    tb.suf_u16(index as u16).add("=> {");

                    if let Some(types) = parser.eat_all_types(){
                        tb.add("std :: result :: Result :: Ok ( Self ::").ident(&variant).add("(");
                        for _ in 0..types.len(){
                            tb.add("DeBin :: de_bin ( o , d ) ? ,");
                        }
                        tb.add(") )");
                    }
                    else if let Some(fields) = parser.eat_all_struct_fields(){ // named variant
                        tb.add("std :: result :: Result :: Ok ( Self ::").ident(&variant).add("{");
                        for (field, _ty) in fields.iter(){
                            tb.ident(field).add(": DeBin :: de_bin ( o , d ) ? ,");
                        }
                        tb.add("} )");
                    }
                    else if parser.is_punct(",") || parser.is_eot(){ // bare variant
                        tb.add("std :: result :: Result :: Ok ( Self ::").ident(&variant).add(")");
                    }
                    else{
                        return parser.unexpected();
                    }
                    
                    tb.add("}");
                    index += 1;
                    parser.eat_punct(",");
                }
                else{
                    return parser.unexpected()
                }
            } 
            tb.add("_ => std :: result :: Result :: Err ( DeBinErr { o : * o , l :");
            tb.unsuf_usize(1).add(", s : d . len ( ) } )");
            tb.add("} } } ;");
            return tb.end();
        }
    }
    return parser.unexpected()
}
*/ 