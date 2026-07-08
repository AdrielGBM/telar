[logic]
let name = signal(String::new());
let query = signal(String::new());

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Text field" desc:"text_field wraps the input primitive in a bordered, padded box (from the components catalogue, not a base tag): an optional label stacks above it, and a muted placeholder shows while the bound value is empty."
    example title:"A labelled field and a placeholder-only field, both bound to their own signal"
        card gap:10
            text_field value:$name label:"Name" placeholder:"Type your name"
            text_field value:$query placeholder:"Search…"
            text "Hello, {$name}" size:14 color:muted
        code_line code:"text_field value:$name label:'Name' placeholder:'Type your name'   (bordered box + label + muted placeholder while empty)"
