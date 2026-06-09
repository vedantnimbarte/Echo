import Reveal from "@/components/ui/Reveal";

export default function PageHero({
  eyebrow,
  title,
  highlight,
  subtitle,
}: {
  eyebrow: string;
  title: string;
  highlight?: string;
  subtitle?: string;
}) {
  return (
    <section className="relative px-6 pt-32 sm:px-10 sm:pt-40">
      <div className="measure-grid absolute inset-0 -z-10 opacity-40" aria-hidden />
      <div className="mx-auto max-w-4xl text-center">
        <Reveal>
          <p className="eyebrow">{eyebrow}</p>
          <h1 className="mt-5 text-balance text-6xl font-semibold leading-[0.95] sm:text-8xl">
            {title} {highlight && <span className="signal-gradient">{highlight}</span>}
          </h1>
          {subtitle && (
            <p className="mx-auto mt-6 max-w-2xl text-balance text-lg leading-relaxed text-fog sm:text-xl">
              {subtitle}
            </p>
          )}
        </Reveal>
      </div>
    </section>
  );
}
