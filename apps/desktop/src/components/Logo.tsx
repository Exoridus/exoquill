/** The exoquill mark: a rounded square outline with a small green accent dot. */
export function LogoMark({ size = 17 }: { size?: number }) {
  const dot = Math.max(3, Math.round(size * 0.24));
  return (
    <div className="logo-mark" style={{ width: size, height: size }}>
      <div className="logo-mark__dot" style={{ width: dot, height: dot }} />
    </div>
  );
}
