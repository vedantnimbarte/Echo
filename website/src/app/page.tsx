import Hero from "@/components/sections/Hero";
import Marquee from "@/components/sections/Marquee";
import FeatureIndex from "@/components/sections/FeatureIndex";
import HowItWorks from "@/components/sections/HowItWorks";
import Privacy from "@/components/sections/Privacy";
import FinalCTA from "@/components/sections/FinalCTA";

export default function Home() {
  return (
    <>
      <Hero />
      <Marquee />
      <FeatureIndex />
      <HowItWorks />
      <Privacy />
      <FinalCTA />
    </>
  );
}
