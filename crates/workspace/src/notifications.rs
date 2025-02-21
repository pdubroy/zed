use crate::{Toast, Workspace};
use collections::HashMap;
use gpui::{
    AnyView, AppContext, AsyncWindowContext, DismissEvent, Entity, EntityId, EventEmitter, Render,
    Task, View, ViewContext, VisualContext, WindowContext,
};
use std::{any::TypeId, ops::DerefMut};

pub fn init(cx: &mut AppContext) {
    cx.set_global(NotificationTracker::new());
}

pub trait Notification: EventEmitter<DismissEvent> + Render {}

impl<V: EventEmitter<DismissEvent> + Render> Notification for V {}

pub trait NotificationHandle: Send {
    fn id(&self) -> EntityId;
    fn to_any(&self) -> AnyView;
}

impl<T: Notification> NotificationHandle for View<T> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }
}

impl From<&dyn NotificationHandle> for AnyView {
    fn from(val: &dyn NotificationHandle) -> Self {
        val.to_any()
    }
}

pub(crate) struct NotificationTracker {
    notifications_sent: HashMap<TypeId, Vec<usize>>,
}

impl std::ops::Deref for NotificationTracker {
    type Target = HashMap<TypeId, Vec<usize>>;

    fn deref(&self) -> &Self::Target {
        &self.notifications_sent
    }
}

impl DerefMut for NotificationTracker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.notifications_sent
    }
}

impl NotificationTracker {
    fn new() -> Self {
        Self {
            notifications_sent: Default::default(),
        }
    }
}

impl Workspace {
    pub fn has_shown_notification_once<V: Notification>(
        &self,
        id: usize,
        cx: &ViewContext<Self>,
    ) -> bool {
        cx.global::<NotificationTracker>()
            .get(&TypeId::of::<V>())
            .map(|ids| ids.contains(&id))
            .unwrap_or(false)
    }

    pub fn show_notification_once<V: Notification>(
        &mut self,
        id: usize,
        cx: &mut ViewContext<Self>,
        build_notification: impl FnOnce(&mut ViewContext<Self>) -> View<V>,
    ) {
        if !self.has_shown_notification_once::<V>(id, cx) {
            let tracker = cx.global_mut::<NotificationTracker>();
            let entry = tracker.entry(TypeId::of::<V>()).or_default();
            entry.push(id);
            self.show_notification::<V>(id, cx, build_notification)
        }
    }

    pub fn show_notification<V: Notification>(
        &mut self,
        id: usize,
        cx: &mut ViewContext<Self>,
        build_notification: impl FnOnce(&mut ViewContext<Self>) -> View<V>,
    ) {
        let type_id = TypeId::of::<V>();
        if self
            .notifications
            .iter()
            .all(|(existing_type_id, existing_id, _)| {
                (*existing_type_id, *existing_id) != (type_id, id)
            })
        {
            let notification = build_notification(cx);
            cx.subscribe(&notification, move |this, _, _: &DismissEvent, cx| {
                this.dismiss_notification_internal(type_id, id, cx);
            })
            .detach();
            self.notifications
                .push((type_id, id, Box::new(notification)));
            cx.notify();
        }
    }

    pub fn show_error<E>(&mut self, err: &E, cx: &mut ViewContext<Self>)
    where
        E: std::fmt::Debug,
    {
        self.show_notification(0, cx, |cx| {
            cx.new_view(|_cx| {
                simple_message_notification::MessageNotification::new(format!("Error: {err:?}"))
            })
        });
    }

    pub fn dismiss_notification<V: Notification>(&mut self, id: usize, cx: &mut ViewContext<Self>) {
        let type_id = TypeId::of::<V>();

        self.dismiss_notification_internal(type_id, id, cx)
    }

    pub fn show_toast(&mut self, toast: Toast, cx: &mut ViewContext<Self>) {
        self.dismiss_notification::<simple_message_notification::MessageNotification>(toast.id, cx);
        self.show_notification(toast.id, cx, |cx| {
            cx.new_view(|_cx| match toast.on_click.as_ref() {
                Some((click_msg, on_click)) => {
                    let on_click = on_click.clone();
                    simple_message_notification::MessageNotification::new(toast.msg.clone())
                        .with_click_message(click_msg.clone())
                        .on_click(move |cx| on_click(cx))
                }
                None => simple_message_notification::MessageNotification::new(toast.msg.clone()),
            })
        })
    }

    pub fn dismiss_toast(&mut self, id: usize, cx: &mut ViewContext<Self>) {
        self.dismiss_notification::<simple_message_notification::MessageNotification>(id, cx);
    }

    fn dismiss_notification_internal(
        &mut self,
        type_id: TypeId,
        id: usize,
        cx: &mut ViewContext<Self>,
    ) {
        self.notifications
            .retain(|(existing_type_id, existing_id, _)| {
                if (*existing_type_id, *existing_id) == (type_id, id) {
                    cx.notify();
                    false
                } else {
                    true
                }
            });
    }
}

pub mod simple_message_notification {
    use gpui::{
        div, DismissEvent, EventEmitter, InteractiveElement, ParentElement, Render, SharedString,
        StatefulInteractiveElement, Styled, ViewContext,
    };
    use std::sync::Arc;
    use ui::prelude::*;
    use ui::{h_flex, v_flex, Button, Icon, IconName, Label, StyledExt};

    pub struct MessageNotification {
        message: SharedString,
        on_click: Option<Arc<dyn Fn(&mut ViewContext<Self>)>>,
        click_message: Option<SharedString>,
    }

    impl EventEmitter<DismissEvent> for MessageNotification {}

    impl MessageNotification {
        pub fn new<S>(message: S) -> MessageNotification
        where
            S: Into<SharedString>,
        {
            Self {
                message: message.into(),
                on_click: None,
                click_message: None,
            }
        }

        pub fn with_click_message<S>(mut self, message: S) -> Self
        where
            S: Into<SharedString>,
        {
            self.click_message = Some(message.into());
            self
        }

        pub fn on_click<F>(mut self, on_click: F) -> Self
        where
            F: 'static + Fn(&mut ViewContext<Self>),
        {
            self.on_click = Some(Arc::new(on_click));
            self
        }

        pub fn dismiss(&mut self, cx: &mut ViewContext<Self>) {
            cx.emit(DismissEvent);
        }
    }

    impl Render for MessageNotification {
        fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
            v_flex()
                .elevation_3(cx)
                .p_4()
                .child(
                    h_flex()
                        .justify_between()
                        .child(div().max_w_80().child(Label::new(self.message.clone())))
                        .child(
                            div()
                                .id("cancel")
                                .child(Icon::new(IconName::Close))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, cx| this.dismiss(cx))),
                        ),
                )
                .children(self.click_message.iter().map(|message| {
                    Button::new(message.clone(), message.clone()).on_click(cx.listener(
                        |this, _, cx| {
                            if let Some(on_click) = this.on_click.as_ref() {
                                (on_click)(cx)
                            };
                            this.dismiss(cx)
                        },
                    ))
                }))
        }
    }
}

pub trait NotifyResultExt {
    type Ok;

    fn notify_err(
        self,
        workspace: &mut Workspace,
        cx: &mut ViewContext<Workspace>,
    ) -> Option<Self::Ok>;

    fn notify_async_err(self, cx: &mut AsyncWindowContext) -> Option<Self::Ok>;
}

impl<T, E> NotifyResultExt for Result<T, E>
where
    E: std::fmt::Debug,
{
    type Ok = T;

    fn notify_err(self, workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("TODO {err:?}");
                workspace.show_error(&err, cx);
                None
            }
        }
    }

    fn notify_async_err(self, cx: &mut AsyncWindowContext) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("TODO {err:?}");
                cx.update_root(|view, cx| {
                    if let Ok(workspace) = view.downcast::<Workspace>() {
                        workspace.update(cx, |workspace, cx| workspace.show_error(&err, cx))
                    }
                })
                .ok();
                None
            }
        }
    }
}

pub trait NotifyTaskExt {
    fn detach_and_notify_err(self, cx: &mut WindowContext);
}

impl<R, E> NotifyTaskExt for Task<Result<R, E>>
where
    E: std::fmt::Debug + 'static,
    R: 'static,
{
    fn detach_and_notify_err(self, cx: &mut WindowContext) {
        cx.spawn(|mut cx| async move { self.await.notify_async_err(&mut cx) })
            .detach();
    }
}
