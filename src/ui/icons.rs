use dioxus::prelude::*;

const ICON_SIZE: &str = "16";
const ICON_VIEW_BOX: &str = "0 0 20 20";

#[component]
pub fn FolderIcon() -> Element {
    rsx! {
        svg {
            class: "picker-icon folder",
            view_box: ICON_VIEW_BOX,
            width: ICON_SIZE,
            height: ICON_SIZE,
            path {
                fill: "currentColor",
                d: "M2 5.5A1.5 1.5 0 0 1 3.5 4h4.09a1.5 1.5 0 0 1 1.06.44L10.2 6H16.5A1.5 1.5 0 0 1 18 7.5v7a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 2 14.5v-9Z",
            }
        }
    }
}

#[component]
pub fn DatabaseIcon() -> Element {
    rsx! {
        svg {
            class: "database-icon",
            view_box: ICON_VIEW_BOX,
            width: "18",
            height: "18",
            path {
                fill: "currentColor",
                d: "M10 2c-3.87 0-7 1.34-7 3v10c0 1.66 3.13 3 7 3s7-1.34 7-3V5c0-1.66-3.13-3-7-3Zm0 1.5c3.2 0 5.5 1.06 5.5 1.5S13.2 6.5 10 6.5 4.5 5.44 4.5 5 6.8 3.5 10 3.5ZM4.5 7.03C5.8 7.65 7.8 8 10 8s4.2-.35 5.5-.97V10c0 .44-2.3 1.5-5.5 1.5S4.5 10.44 4.5 10V7.03Zm0 5C5.8 12.65 7.8 13 10 13s4.2-.35 5.5-.97V15c0 .44-2.3 1.5-5.5 1.5S4.5 15.44 4.5 15v-2.97Z",
            }
        }
    }
}

#[component]
pub fn ServerIcon() -> Element {
    rsx! {
        svg {
            class: "picker-icon server",
            view_box: ICON_VIEW_BOX,
            width: ICON_SIZE,
            height: ICON_SIZE,
            path {
                fill: "currentColor",
                d: "M3 4.5A1.5 1.5 0 0 1 4.5 3h11A1.5 1.5 0 0 1 17 4.5v3A1.5 1.5 0 0 1 15.5 9h-11A1.5 1.5 0 0 1 3 7.5v-3ZM14 6a1 1 0 1 0-2 0 1 1 0 0 0 2 0ZM3 12.5A1.5 1.5 0 0 1 4.5 11h11a1.5 1.5 0 0 1 1.5 1.5v3a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 15.5v-3ZM14 14a1 1 0 1 0-2 0 1 1 0 0 0 2 0Z",
            }
        }
    }
}

#[component]
pub fn PencilIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: "14",
            height: "14",
            path {
                fill: "currentColor",
                d: "M13.6 2.6a2 2 0 0 1 2.8 0l1 1a2 2 0 0 1 0 2.8l-9.2 9.2a1 1 0 0 1-.46.26l-3.5.9a.75.75 0 0 1-.9-.9l.9-3.5a1 1 0 0 1 .26-.46l9.1-9.3Zm1.74 1.06a.5.5 0 0 0-.7 0L13.5 4.8l1.7 1.7 1.14-1.14a.5.5 0 0 0 0-.7l-1-1ZM14.14 7.56l-1.7-1.7-7.3 7.32-.56 2.17 2.17-.56 7.4-7.23Z",
            }
        }
    }
}

#[component]
pub fn ProfileIcon() -> Element {
    rsx! {
        svg {
            class: "picker-icon profile",
            view_box: ICON_VIEW_BOX,
            width: ICON_SIZE,
            height: ICON_SIZE,
            path {
                fill: "currentColor",
                d: "M10 10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm-6.5 7a6.5 6.5 0 0 1 13 0 .75.75 0 0 1-.75.75H4.25A.75.75 0 0 1 3.5 17Z",
            }
        }
    }
}

#[component]
pub fn SearchIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: ICON_SIZE,
            height: ICON_SIZE,
            path {
                fill: "currentColor",
                d: "M8.5 3a5.5 5.5 0 0 1 4.38 8.82l3.65 3.65a.75.75 0 1 1-1.06 1.06l-3.65-3.65A5.5 5.5 0 1 1 8.5 3Zm0 1.5a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z",
            }
        }
    }
}

#[component]
pub fn CloudIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: "18",
            height: "18",
            path {
                fill: "currentColor",
                d: "M10.5 4a5 5 0 0 1 4.84 3.76A4 4 0 0 1 14.5 15.5h-9a3.5 3.5 0 0 1-.77-6.92A5 5 0 0 1 10.5 4Zm0 1.5a3.5 3.5 0 0 0-3.46 2.98.75.75 0 0 1-.64.63A2 2 0 0 0 5.5 14h9a2.5 2.5 0 0 0 .28-4.99.75.75 0 0 1-.66-.6A3.5 3.5 0 0 0 10.5 5.5Z",
            }
        }
    }
}

#[component]
pub fn DownloadIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: "18",
            height: "18",
            path {
                fill: "currentColor",
                d: "M10 2.75a.75.75 0 0 1 .75.75v7.69l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3.75 3.75a.75.75 0 0 1-1.06 0L5.72 9.78a.75.75 0 0 1 1.06-1.06l2.47 2.47V3.5a.75.75 0 0 1 .75-.75ZM4 14.25a.75.75 0 0 1 .75.75v.5c0 .14.11.25.25.25h10a.25.25 0 0 0 .25-.25V15a.75.75 0 0 1 1.5 0v.5A1.75 1.75 0 0 1 15 17.25H5a1.75 1.75 0 0 1-1.75-1.75V15a.75.75 0 0 1 .75-.75Z",
            }
        }
    }
}

#[component]
pub fn RefreshIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: "18",
            height: "18",
            path {
                fill: "currentColor",
                d: "M15.3 4.7A7 7 0 0 0 3.1 8.6a.75.75 0 1 0 1.48.25A5.5 5.5 0 0 1 14 5.6h-1.75a.75.75 0 0 0 0 1.5h3.5A.75.75 0 0 0 16.5 6.35v-3.5a.75.75 0 0 0-1.5 0v1.85ZM16.3 10.9a.75.75 0 0 0-.87.6A5.5 5.5 0 0 1 6 14.4h1.75a.75.75 0 0 0 0-1.5h-3.5a.75.75 0 0 0-.75.75v3.5a.75.75 0 0 0 1.5 0v-1.85a7 7 0 0 0 12.2-3.93.75.75 0 0 0-.6-.87Z",
            }
        }
    }
}

#[component]
pub fn PlusIcon() -> Element {
    rsx! {
        svg {
            view_box: ICON_VIEW_BOX,
            width: "14",
            height: "14",
            path {
                fill: "currentColor",
                d: "M10 3.5a.9.9 0 0 1 .9.9v4.7h4.7a.9.9 0 0 1 0 1.8h-4.7v4.7a.9.9 0 0 1-1.8 0v-4.7H4.4a.9.9 0 0 1 0-1.8h4.7V4.4a.9.9 0 0 1 .9-.9Z",
            }
        }
    }
}
