[logic]
// A bound bool for the checkbox, another for the toggle, and a u32 for the radio group's selection.
let agree = signal(false);
let notify = signal(true);
let choice = signal(0u32);

[view]
col gap:20
    doc_header kicker:"15 · INTERACTION" title:"Form controls" desc:"checkbox, toggle, and radio are components (from the components feature, not base tags). Each binds a signal two-way: the widget renders from it and a tap writes it back. A radio group is several radios sharing one signal, each with its own value."
    col gap:8
        text "checkbox — a bound bool; tap the box or its label to toggle" size:13 color:ink
        card gap:10
            checkbox checked:$agree label:"I agree to the terms"
            text "agree · {$agree}" size:14 color:muted
        code_line code:"checkbox checked:$agree label:'I agree to the terms'"
    col gap:8
        text "toggle — a switch over the same kind of bool signal" size:13 color:ink
        card gap:10
            toggle checked:$notify label:"Email notifications"
            text "notify · {$notify}" size:14 color:muted
        code_line code:"toggle checked:$notify label:'Email notifications'"
    col gap:8
        text "radio — several buttons share one signal; each carries a distinct value" size:13 color:ink
        card gap:10
            radio selected:$choice value:0u32 label:"Small"
            radio selected:$choice value:1u32 label:"Medium"
            radio selected:$choice value:2u32 label:"Large"
            text "choice · {$choice}" size:14 color:muted
        code_line code:"radio selected:$choice value:0u32 label:'Small'      (a group shares one signal)"
    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"checked" values:"signal" about:"Bound bool for checkbox/toggle; the tap writes it back."
            prop_row name:"selected" values:"signal" about:"Bound u32 shared by a radio group; a tap sets it to this button's value."
            prop_row name:"value" values:"u32" about:"The value a radio selects when tapped (which button is on)."
            prop_row name:"on_toggle / on_select" values:"closure" about:"Fires with the new state when it changes."
