[logic]
let tab = signal(0u32);
let open = signal(true);

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Tabs & accordion" desc:"tabs is a bound selected-index bar; pair it with reactive ifs to swap panels. accordion is an inline collapsible section that pushes its siblings as it opens. Both are components."
    example title:"tabs — a bound index; swap panels with reactive ifs"
        card gap:10
            tabs selected:$tab items:vec!["Overview","Pricing","Team"]
            if $tab == 0
                text "Overview — what the product does." size:14 color:ink
            if $tab == 1
                text "Pricing — plans and limits." size:14 color:ink
            if $tab == 2
                text "Team — who is behind it." size:14 color:ink
        code_line code:"tabs selected:$tab items:vec!['Overview','Pricing','Team']"
    example title:"accordion — a collapsible section, open bound to a signal"
        card gap:10
            accordion title:"Shipping details" open:$open
                text "Ships in 2–3 business days. Free over $50." size:14 color:muted
            text "open · {$open}" size:13 color:muted
        code_line code:"accordion title:'Shipping details' open:$open  >  …body…"
    example title:"Attributes"
        col gap:6
            prop_row name:"items" values:"vec![..]" about:"tabs labels, one button each."
            prop_row name:"selected" values:"signal" about:"tabs active index (u32), two-way."
            prop_row name:"title" values:"text" about:"accordion header label."
            prop_row name:"open" values:"signal" about:"accordion expanded bool, two-way."
