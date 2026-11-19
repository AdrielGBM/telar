# Navigation

The [`telar-navigate`](../plugins/telar-navigate) plugin holds two route stacks. `Navigator` and `NavHost` are one
stack shown as a page; `TabStacks` and `TabHost` are one stack per tab, which is how a tab bar keeps a tab you
left several screens deep until you come back to it. How a stack follows the app's address is in
[docs/location.md](location.md).

## Tabs

```rust
use telar_navigate::{Navigator, PagePolicy, TabHost, TabStacks};

let stacks = TabStacks::new(active, &[Tab::Today, Tab::Reading], |tab| Navigator::new(tab.root()));
let pages = TabHost::new(stacks.clone(), |_tab, page| build_page(page))?
    .with_policy(PagePolicy::Transient);
```

`TabStacks` is the state a tab bar drives (`select`, `back`, `push`, `navigator_for`); `TabHost` shows the
active tab. Each tab's pages are built on its first visit and kept while another tab is on screen, collapsed out
of the layout.

## A frame around a tab's pages

Some tabs show more than their page stack: an explorer beside a reader's pages, a toolbar over a list's. That
UI belongs to the tab rather than to any one page, so it is built once, beside the stack, and stays as the reader
navigates inside the tab. `with_frame` names the tab and builds what it shows around its pages, handed to it as
`Children`:

```rust
let pages = TabHost::new(stacks.clone(), |_tab, page| build_page(page))?
    .with_policy(PagePolicy::Transient)
    .with_frame(Tab::Reading, |pages| {
        explorer_frame(ExplorerFrameProps::props().children(pages).build())
    })?;
```

```rsx
[logic]
pub struct Props {
    #[props(default)]
    pub children: Children,
}

let pages = props.children;

[view]
row grow:1 min_height:0
    explorer
    col grow:1 min_height:0
        children pages
```

- **Built once, for the tab's life.** The frame is built on the tab's first visit, under an owner that lives as
  long as the tab, so its state (a selection, a width, a scroll) is kept across every navigation inside the tab
  and while another tab is on screen. A tab with no frame shows its pages alone.
- **The pages are built once too.** A frame that places its pages again, as one that rearranges itself across a
  breakpoint does (`if $docked` a split pane, `else` a drawer), moves them to the newest placement still
  standing rather than building them again: the page on top, its scroll and its fields stay as they were.
- **Set at any time.** A frame for a tab already built, such as the active one `TabHost::new` builds, is built
  at once; a second frame for the same tab replaces the first. A frame that fails to build is an error from
  `with_frame` for a tab already built, and is logged on a later first visit as a page that fails to build is.
- **Every target.** The frame is ordinary UI in the tab's place, so it lays out, draws, takes events and is read
  by a reader the same way on every target.
