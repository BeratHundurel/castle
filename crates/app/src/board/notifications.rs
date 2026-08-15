use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationAvailability {
    Enabled,
    DisabledForApplication,
    DisabledForUser,
    DisabledByPolicy,
    Unsupported,
    Unavailable,
}

impl NotificationAvailability {
    pub(super) fn can_open_settings(self) -> bool {
        matches!(self, Self::DisabledForApplication | Self::DisabledForUser)
    }
}

pub(crate) trait NotificationGateway: Send + Sync + 'static {
    fn availability(&self) -> NotificationAvailability;
    fn wake(&self);
    fn show_test_notification(&self) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub(super) struct BoardNotifications {
    gateway: Arc<dyn NotificationGateway>,
}

impl BoardNotifications {
    pub(super) fn new(gateway: Arc<dyn NotificationGateway>) -> Self {
        Self { gateway }
    }

    pub(super) fn unavailable() -> Self {
        Self::new(Arc::new(UnavailableNotificationGateway))
    }

    pub(super) fn availability(&self) -> NotificationAvailability {
        self.gateway.availability()
    }

    pub(super) fn wake(&self) {
        self.gateway.wake();
    }

    pub(super) fn show_test_notification(&self) -> anyhow::Result<()> {
        self.gateway.show_test_notification()
    }
}

struct UnavailableNotificationGateway;

impl NotificationGateway for UnavailableNotificationGateway {
    fn availability(&self) -> NotificationAvailability {
        NotificationAvailability::Unsupported
    }

    fn wake(&self) {}

    fn show_test_notification(&self) -> anyhow::Result<()> {
        anyhow::bail!("system notifications are not supported on this platform")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct FakeNotificationGateway {
        availability: NotificationAvailability,
        wake_count: AtomicUsize,
        test_notification_count: AtomicUsize,
        test_notification_fails: bool,
    }

    impl FakeNotificationGateway {
        fn new(availability: NotificationAvailability) -> Self {
            Self {
                availability,
                wake_count: AtomicUsize::new(0),
                test_notification_count: AtomicUsize::new(0),
                test_notification_fails: false,
            }
        }

        fn failing() -> Self {
            Self {
                test_notification_fails: true,
                ..Self::new(NotificationAvailability::Enabled)
            }
        }
    }

    impl NotificationGateway for FakeNotificationGateway {
        fn availability(&self) -> NotificationAvailability {
            self.availability
        }

        fn wake(&self) {
            self.wake_count.fetch_add(1, Ordering::Relaxed);
        }

        fn show_test_notification(&self) -> anyhow::Result<()> {
            self.test_notification_count.fetch_add(1, Ordering::Relaxed);
            if self.test_notification_fails {
                anyhow::bail!("fake notification failure");
            }
            Ok(())
        }
    }

    #[test]
    fn availability_comes_from_the_injected_gateway() {
        let gateway = Arc::new(FakeNotificationGateway::new(
            NotificationAvailability::DisabledForApplication,
        ));
        let notifications = BoardNotifications::new(gateway);

        assert_eq!(
            notifications.availability(),
            NotificationAvailability::DisabledForApplication
        );
    }

    #[test]
    fn wake_is_forwarded_to_the_injected_gateway() {
        let gateway = Arc::new(FakeNotificationGateway::new(
            NotificationAvailability::Enabled,
        ));
        let notifications = BoardNotifications::new(gateway.clone());

        notifications.wake();
        notifications.wake();

        assert_eq!(gateway.wake_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_notification_preserves_gateway_success_and_failure() {
        let successful_gateway = Arc::new(FakeNotificationGateway::new(
            NotificationAvailability::Enabled,
        ));
        let successful_notifications = BoardNotifications::new(successful_gateway.clone());
        successful_notifications
            .show_test_notification()
            .expect("fake notification should succeed");
        assert_eq!(
            successful_gateway
                .test_notification_count
                .load(Ordering::Relaxed),
            1
        );

        let failing_gateway = Arc::new(FakeNotificationGateway::failing());
        let failing_notifications = BoardNotifications::new(failing_gateway.clone());
        let error = failing_notifications
            .show_test_notification()
            .expect_err("fake notification should fail");
        assert_eq!(error.to_string(), "fake notification failure");
        assert_eq!(
            failing_gateway
                .test_notification_count
                .load(Ordering::Relaxed),
            1
        );
    }
}
