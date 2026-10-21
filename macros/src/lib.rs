use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    braced, bracketed, parenthesized, parse_macro_input, Attribute, Error, Ident, LitInt, LitStr,
    Result, Token,
};

/// A complete `define_vars! { ... }` invocation.
struct DefineVarsInput {
    defs: Vec<VarDef>,
}

struct VarDef {
    attrs: Vec<Attribute>,
    name: Ident,
    arity: usize,
    arity_span: Span,
    setup_blocks: Vec<SetupBlock>,
    final_block: Option<BlockCall>,
    final_code: Option<Template>,
}

struct SetupBlock {
    temp_idx: usize,
    temp_span: Span,
    call: BlockCall,
}

#[derive(Clone)]
struct BlockCall {
    action: Ident,
    args: Vec<BlockArg>,
    tags: Vec<Ident>,
}

#[derive(Clone)]
enum BlockArg {
    Param { idx: usize, span: Span },
    Temp { idx: usize, span: Span },
    Template(Template),
}

#[derive(Clone)]
struct Template {
    segments: Vec<TemplateSegment>,
    span: Span,
}

#[derive(Clone)]
enum TemplateSegment {
    Literal(String),
    Param { idx: usize, span: Span },
    Temp { idx: usize, span: Span },
}

impl Parse for DefineVarsInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut defs = Vec::new();
        while !input.is_empty() {
            defs.push(input.parse::<VarDef>()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(DefineVarsInput { defs })
    }
}

impl Parse for VarDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;

        let arity_content;
        parenthesized!(arity_content in input);
        let arity_lit: LitInt = arity_content.parse()?;
        let arity: usize = arity_lit.base10_parse()?;
        let arity_span = arity_lit.span();

        input.parse::<Token![=>]>()?;

        let mut setup_blocks = Vec::new();
        let mut final_block = None;
        let mut final_code = None;

        if input.peek(syn::token::Brace) {
            let body;
            braced!(body in input);

            // Parse 0 or more `#k = Action(...) [...];` setup blocks
            while body.peek(Token![#]) {
                body.parse::<Token![#]>()?;
                let temp_lit: LitInt = body.parse()?;
                let temp_idx: usize = temp_lit.base10_parse()?;
                let temp_span = temp_lit.span();
                body.parse::<Token![=]>()?;
                let call: BlockCall = body.parse()?;
                body.parse::<Token![;]>()?;
                setup_blocks.push(SetupBlock {
                    temp_idx,
                    temp_span,
                    call,
                });
            }

            // Parse final block and/or final var-code string
            if body.peek(LitStr) {
                let lit: LitStr = body.parse()?;
                final_code = Some(parse_template(&lit)?);
                if body.peek(Token![;]) {
                    body.parse::<Token![;]>()?;
                }
            } else if body.peek(Ident) {
                final_block = Some(body.parse::<BlockCall>()?);
                if body.peek(Token![;]) {
                    body.parse::<Token![;]>()?;
                }
                if body.peek(LitStr) {
                    let lit: LitStr = body.parse()?;
                    final_code = Some(parse_template(&lit)?);
                    if body.peek(Token![;]) {
                        body.parse::<Token![;]>()?;
                    }
                }
            } else {
                return Err(body.error(
                    "expected a final codeblock `Action(...)` or var-code string literal `\"...\"`",
                ));
            }

            if !body.is_empty() {
                return Err(body.error("unexpected tokens after final step of instruction"));
            }
        } else if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            final_code = Some(parse_template(&lit)?);
        } else if input.peek(Ident) {
            final_block = Some(input.parse::<BlockCall>()?);
        } else {
            return Err(input.error(
                "expected `{ ... }`, a codeblock `Action(...)`, or a var-code string `\"...\"`",
            ));
        }

        if final_block.is_none() && final_code.is_none() {
            return Err(Error::new(
                name.span(),
                "instruction must specify a final codeblock, a var-code string, or both",
            ));
        }

        Ok(VarDef {
            attrs,
            name,
            arity,
            arity_span,
            setup_blocks,
            final_block,
            final_code,
        })
    }
}

impl Parse for BlockCall {
    fn parse(input: ParseStream) -> Result<Self> {
        let action: Ident = input.parse()?;

        let args_content;
        parenthesized!(args_content in input);
        let mut args = Vec::new();
        while !args_content.is_empty() {
            args.push(args_content.parse::<BlockArg>()?);
            if args_content.peek(Token![,]) {
                args_content.parse::<Token![,]>()?;
            } else if !args_content.is_empty() {
                return Err(args_content.error("expected `,` between codeblock arguments"));
            }
        }

        let mut tags = Vec::new();
        if input.peek(syn::token::Bracket) {
            let tags_content;
            bracketed!(tags_content in input);
            while !tags_content.is_empty() {
                tags.push(tags_content.parse::<Ident>()?);
                if tags_content.peek(Token![,]) {
                    tags_content.parse::<Token![,]>()?;
                } else if !tags_content.is_empty() {
                    return Err(tags_content.error("expected `,` between tags"));
                }
            }
        }

        Ok(BlockCall { action, args, tags })
    }
}

impl Parse for BlockArg {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![$]) {
            input.parse::<Token![$]>()?;
            let lit: LitInt = input.parse()?;
            Ok(BlockArg::Param {
                idx: lit.base10_parse()?,
                span: lit.span(),
            })
        } else if input.peek(Token![#]) {
            input.parse::<Token![#]>()?;
            let lit: LitInt = input.parse()?;
            Ok(BlockArg::Temp {
                idx: lit.base10_parse()?,
                span: lit.span(),
            })
        } else if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            Ok(BlockArg::Template(parse_template(&lit)?))
        } else {
            Err(input.error(
                "expected parameter `$n`, temporary variable `#k`, or string literal `\"...\"`",
            ))
        }
    }
}

fn parse_template(lit: &LitStr) -> Result<Template> {
    let raw = lit.value();
    let span = lit.span();
    let bytes = raw.as_bytes();
    let mut segments = Vec::new();
    let mut cur_lit = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'$' || b == b'#' {
            if i + 1 < bytes.len() && bytes[i + 1] == b {
                // Escaped `$$` or `##`
                cur_lit.push(b as char);
                i += 2;
                continue;
            }
            if !cur_lit.is_empty() {
                segments.push(TemplateSegment::Literal(std::mem::take(&mut cur_lit)));
            }
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if start == i {
                return Err(Error::new(
                    span,
                    format!(
                        "expected digits after `{}` in template string (use `{}{}` to escape)",
                        b as char, b as char, b as char
                    ),
                ));
            }
            let idx: usize = raw[start..i]
                .parse()
                .map_err(|e| Error::new(span, format!("invalid variable index: {e}")))?;
            if b == b'$' {
                segments.push(TemplateSegment::Param { idx, span });
            } else {
                segments.push(TemplateSegment::Temp { idx, span });
            }
        } else {
            let ch = raw[i..].chars().next().unwrap();
            cur_lit.push(ch);
            i += ch.len_utf8();
        }
    }

    if !cur_lit.is_empty() {
        segments.push(TemplateSegment::Literal(cur_lit));
    }

    Ok(Template { segments, span })
}

#[derive(Copy, Clone)]
enum Ending<'a> {
    Block(&'a BlockCall),
    Code(&'a Template),
}

struct UsageCounts {
    param_total: Vec<usize>,
    temp_total: Vec<usize>,
}

struct UseTracker {
    param_total: Vec<usize>,
    param_seen: Vec<usize>,
    temp_total: Vec<usize>,
    temp_seen: Vec<usize>,
}

impl UseTracker {
    fn new(counts: UsageCounts) -> Self {
        let param_len = counts.param_total.len();
        let temp_len = counts.temp_total.len();
        Self {
            param_total: counts.param_total,
            param_seen: vec![0; param_len],
            temp_total: counts.temp_total,
            temp_seen: vec![0; temp_len],
        }
    }

    /// Generate an `ItemData` expression for a parameter `$idx` in a codeblock slot.
    fn gen_param_item(&mut self, idx: usize) -> TokenStream2 {
        let total = self.param_total[idx];
        self.param_seen[idx] += 1;
        let seen = self.param_seen[idx];
        if total == 1 {
            quote! { self.make_item(&v[#idx]) }
        } else {
            let ident = format_ident!("p_{}", idx);
            if seen == total {
                quote! { self.true_item(#ident) }
            } else {
                quote! { #ident.to_item() }
            }
        }
    }

    /// Generate a string-formatting statement for a parameter `$idx` inside a template.
    fn gen_param_code(&mut self, idx: usize, buf: &TokenStream2) -> TokenStream2 {
        let total = self.param_total[idx];
        self.param_seen[idx] += 1;
        let seen = self.param_seen[idx];
        if total == 1 {
            quote! { self.var_code(#buf, &v[#idx]); }
        } else {
            let ident = format_ident!("p_{}", idx);
            if seen == total {
                quote! { self.true_code(#buf, #ident); }
            } else {
                quote! { #ident.to_code(#buf); }
            }
        }
    }

    /// Generate an `ItemData` expression for a temporary variable `#idx` in a codeblock slot.
    fn gen_temp_item(&mut self, idx: usize) -> TokenStream2 {
        let total = self.temp_total[idx];
        self.temp_seen[idx] += 1;
        let seen = self.temp_seen[idx];
        let ident = format_ident!("t_{}", idx);
        if seen == total {
            quote! { self.true_item(#ident) }
        } else {
            quote! { #ident.to_item() }
        }
    }

    /// Generate a string-formatting statement for a temporary variable `#idx` inside a template.
    fn gen_temp_code(&mut self, idx: usize, buf: &TokenStream2) -> TokenStream2 {
        let total = self.temp_total[idx];
        self.temp_seen[idx] += 1;
        let seen = self.temp_seen[idx];
        let ident = format_ident!("t_{}", idx);
        if seen == total {
            quote! { self.true_code(#buf, #ident); }
        } else {
            quote! { #ident.to_code(#buf); }
        }
    }
}

fn analyze_path(
    def: &VarDef,
    ending: Ending<'_>,
) -> Result<UsageCounts> {
    let mut param_total = vec![0usize; def.arity];
    let mut temp_total = vec![0usize; def.setup_blocks.len()];

    let mut record_param = |idx: usize, span: Span| -> Result<()> {
        if idx >= def.arity {
            return Err(Error::new(
                span,
                format!(
                    "parameter `${idx}` is out of bounds for instruction `{}` with arity {}",
                    def.name, def.arity
                ),
            ));
        }
        param_total[idx] += 1;
        Ok(())
    };

    let record_temp = |idx: usize, defined_temps: usize, temp_total: &mut [usize], span: Span| -> Result<()> {
        if idx >= defined_temps {
            return Err(Error::new(
                span,
                format!(
                    "temporary variable `#{idx}` is used before it is defined (only {} temporary variable(s) defined at this point)",
                    defined_temps
                ),
            ));
        }
        temp_total[idx] += 1;
        Ok(())
    };

    for (expected_idx, setup) in def.setup_blocks.iter().enumerate() {
        if setup.temp_idx != expected_idx {
            return Err(Error::new(
                setup.temp_span,
                format!(
                    "expected temporary variable `#{expected_idx}` to be defined next, found `#{}`",
                    setup.temp_idx
                ),
            ));
        }
        for arg in &setup.call.args {
            match arg {
                BlockArg::Param { idx, span } => record_param(*idx, *span)?,
                BlockArg::Temp { idx, span } => {
                    record_temp(*idx, expected_idx, &mut temp_total, *span)?
                }
                BlockArg::Template(tmpl) => {
                    for seg in &tmpl.segments {
                        match seg {
                            TemplateSegment::Literal(_) => {}
                            TemplateSegment::Param { idx, span } => record_param(*idx, *span)?,
                            TemplateSegment::Temp { idx, span } => {
                                record_temp(*idx, expected_idx, &mut temp_total, *span)?
                            }
                        }
                    }
                }
            }
        }
    }

    let total_defined_temps = def.setup_blocks.len();
    match ending {
        Ending::Block(call) => {
            for arg in &call.args {
                match arg {
                    BlockArg::Param { idx, span } => record_param(*idx, *span)?,
                    BlockArg::Temp { idx, span } => {
                        record_temp(*idx, total_defined_temps, &mut temp_total, *span)?
                    }
                    BlockArg::Template(tmpl) => {
                        for seg in &tmpl.segments {
                            match seg {
                                TemplateSegment::Literal(_) => {}
                                TemplateSegment::Param { idx, span } => record_param(*idx, *span)?,
                                TemplateSegment::Temp { idx, span } => {
                                    record_temp(*idx, total_defined_temps, &mut temp_total, *span)?
                                }
                            }
                        }
                    }
                }
            }
        }
        Ending::Code(tmpl) => {
            for seg in &tmpl.segments {
                match seg {
                    TemplateSegment::Literal(_) => {}
                    TemplateSegment::Param { idx, span } => record_param(*idx, *span)?,
                    TemplateSegment::Temp { idx, span } => {
                        record_temp(*idx, total_defined_temps, &mut temp_total, *span)?
                    }
                }
            }
        }
    }

    for (idx, &count) in param_total.iter().enumerate() {
        if count == 0 {
            return Err(Error::new(
                def.arity_span,
                format!(
                    "parameter `${idx}` is never used in instruction `{}`",
                    def.name
                ),
            ));
        }
    }

    for (idx, &count) in temp_total.iter().enumerate() {
        if count == 0 {
            return Err(Error::new(
                def.setup_blocks[idx].temp_span,
                format!(
                    "temporary variable `#{idx}` is defined but never used in instruction `{}`",
                    def.name
                ),
            ));
        }
    }

    Ok(UsageCounts {
        param_total,
        temp_total,
    })
}

fn gen_template_stmts(tmpl: &Template, buf: &TokenStream2, tracker: &mut UseTracker) -> TokenStream2 {
    let stmts = tmpl.segments.iter().map(|seg| match seg {
        TemplateSegment::Literal(s) => quote! { (#buf).push_str(#s); },
        TemplateSegment::Param { idx, .. } => tracker.gen_param_code(*idx, buf),
        TemplateSegment::Temp { idx, .. } => tracker.gen_temp_code(*idx, buf),
    });
    quote! { #(#stmts)* }
}

fn gen_block_arg_item(arg: &BlockArg, tracker: &mut UseTracker) -> TokenStream2 {
    match arg {
        BlockArg::Param { idx, .. } => tracker.gen_param_item(*idx),
        BlockArg::Temp { idx, .. } => tracker.gen_temp_item(*idx),
        BlockArg::Template(tmpl) => {
            // Optimization: if the template is a single plain literal, avoid `var_code` calls.
            if tmpl.segments.len() == 1 {
                if let TemplateSegment::Literal(s) = &tmpl.segments[0] {
                    return quote! {
                        ItemData::Number { value: String::from(#s) }
                    };
                }
            }
            let buf = Ident::new("__tmpl_val", tmpl.span);
            let buf_ref = quote! { &mut #buf };
            let stmts = gen_template_stmts(tmpl, &buf_ref, tracker);
            quote! {
                {
                    let mut #buf = String::new();
                    #stmts
                    ItemData::Number { value: #buf }
                }
            }
        }
    }
}

fn gen_block_emission(
    call: &BlockCall,
    dest_expr: TokenStream2,
    tracker: &mut UseTracker,
) -> TokenStream2 {
    let action = &call.action;
    let total_slots = call.args.len() + 1 + call.tags.len();

    let input_slots = call.args.iter().enumerate().map(|(i, arg)| {
        let slot = (i + 1) as u32;
        let item_expr = gen_block_arg_item(arg, tracker);
        quote! {
            vec.push(ChestSlot { slot: #slot, item: #item_expr });
        }
    });

    let tag_slots = call.tags.iter().enumerate().map(|(i, tag)| {
        let slot = (27 - call.tags.len() + i) as u32;
        quote! {
            vec.push(ChestSlot { slot: #slot, item: ItemData::Tag(Tag::#tag) });
        }
    });

    quote! {
        let __mark = self.regs.mark_frees();
        let mut vec = Vec::with_capacity(#total_slots);
        #(#input_slots)*
        self.regs.flush_frees(__mark);
        vec.push(ChestSlot { slot: 0, item: #dest_expr });
        #(#tag_slots)*
        self.line.push(var_block(VarAction::#action, chest_args(vec)));
    }
}

fn gen_path_code(def: &VarDef, ending: Ending<'_>) -> Result<TokenStream2> {
    let counts = analyze_path(def, ending)?;
    let mut tracker = UseTracker::new(counts);

    // Cache any parameter used more than once
    let cache_stmts = tracker
        .param_total
        .iter()
        .enumerate()
        .filter(|&(_, &count)| count > 1)
        .map(|(idx, _)| {
            let ident = format_ident!("p_{}", idx);
            quote! {
                let #ident = self.cache(&v[#idx]);
            }
        })
        .collect::<Vec<_>>();

    // Emit setup blocks
    let mut setup_stmts = Vec::with_capacity(def.setup_blocks.len());
    for setup in &def.setup_blocks {
        let temp_ident = format_ident!("t_{}", setup.temp_idx);
        let dest_expr = quote! {
            {
                let reg = self.regs.alloc_reg();
                #temp_ident = TrueVariable::Register(reg);
                #temp_ident.to_item()
            }
        };
        let block_code = gen_block_emission(&setup.call, dest_expr, &mut tracker);
        setup_stmts.push(quote! {
            let #temp_ident: TrueVariable;
            {
                #block_code
            }
        });
    }

    let final_stmt = match ending {
        Ending::Block(call) => {
            let block_code = gen_block_emission(call, quote! { dest(self) }, &mut tracker);
            quote! {
                {
                    #block_code
                }
            }
        }
        Ending::Code(tmpl) => {
            let f_expr = quote! { f };
            gen_template_stmts(tmpl, &f_expr, &mut tracker)
        }
    };

    Ok(quote! {
        #(#cache_stmts)*
        #(#setup_stmts)*
        #final_stmt
    })
}

#[proc_macro]
pub fn define_vars(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DefineVarsInput);

    let mut enum_variants = Vec::with_capacity(parsed.defs.len());
    let mut var_code_arms = Vec::new();
    let mut assign_arms = Vec::new();

    for def in &parsed.defs {
        let attrs = &def.attrs;
        let name = &def.name;
        let arity = def.arity;
        enum_variants.push(quote! {
            #(#attrs)*
            #name(Box<[Variable; #arity]>)
        });

        if let Some(code_tmpl) = &def.final_code {
            match gen_path_code(def, Ending::Code(code_tmpl)) {
                Ok(body) => {
                    var_code_arms.push(quote! {
                        #(#attrs)*
                        Variable::#name(v) => {
                            #body
                        }
                    });
                }
                Err(err) => return err.to_compile_error().into(),
            }
        }

        if let Some(block_call) = &def.final_block {
            match gen_path_code(def, Ending::Block(block_call)) {
                Ok(body) => {
                    assign_arms.push(quote! {
                        #(#attrs)*
                        Variable::#name(v) => {
                            #body
                        }
                    });
                }
                Err(err) => return err.to_compile_error().into(),
            }
        }
    }

    let expanded = quote! {
        #[derive(Debug, Clone)]
        pub enum Variable {
            TrueVariable(TrueVariable),
            #(#enum_variants,)*
        }

        impl FnState {
            fn var_code(&mut self, f: &mut String, var: &Variable) {
                #[allow(unreachable_patterns)]
                match var {
                    &Variable::TrueVariable(x) => self.true_code(f, x),
                    #(#var_code_arms,)*
                    other => {
                        let cached = self.cache(other);
                        self.true_code(f, cached);
                    }
                }
            }

            fn assign_inner(&mut self, mut dest: impl FnMut(&mut Self) -> ItemData, src: &Variable) {
                #[allow(unreachable_patterns)]
                match src {
                    &Variable::TrueVariable(src) => {
                        let __mark = self.regs.mark_frees();
                        let item = self.true_item(src);
                        self.regs.flush_frees(__mark);
                        let args = chest_args(vec![
                            ChestSlot { slot: 0, item: dest(self) },
                            ChestSlot { slot: 1, item },
                        ]);
                        self.line.push(var_block(VarAction::Set, args));
                    },
                    #(#assign_arms,)*
                    other => {
                        let __mark = self.regs.mark_frees();
                        let item = self.make_item(other);
                        self.regs.flush_frees(__mark);
                        let args = chest_args(vec![
                            ChestSlot { slot: 0, item: dest(self) },
                            ChestSlot { slot: 1, item },
                        ]);
                        self.line.push(var_block(VarAction::Set, args));
                    }
                }
            }
        }
    };

    expanded.into()
}
