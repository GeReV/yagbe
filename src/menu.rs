use tao::menu::{MenuBar, MenuId, MenuItem, MenuItemAttributes};

pub(crate) const MENU_OPEN: MenuId = MenuId(1);

pub(crate) fn build_menu() -> MenuBar {
    let mut root = MenuBar::new();
    let mut file_menu = MenuBar::new();

    file_menu.add_item(MenuItemAttributes::new("&Open ROM file...").with_id(MENU_OPEN));
    file_menu.add_native_item(MenuItem::Separator);
    file_menu.add_native_item(MenuItem::Quit);
    root.add_submenu("&File", true, file_menu);

    return root;
}
