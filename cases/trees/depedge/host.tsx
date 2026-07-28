import dynamic from 'next/dynamic';
const Chart = dynamic(() => import('./chart'));
export const Host = Chart;
