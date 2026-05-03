pub(crate) fn try_map<F, T: ?Sized, U: ?Sized>(mut t: &mut T, f: F) -> Result<&mut U, &mut T>
where
    F: FnOnce(&mut T) -> Option<&mut U>,
{
    use polonius_the_crab::{polonius, polonius_return};
    polonius!(|t| -> Result<&'polonius mut U, &mut T> {
        if let Some(u) = f(t) {
            polonius_return!(Ok(u));
        }
    });
    Err(t)
}
